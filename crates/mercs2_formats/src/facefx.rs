//! FaceFX archives: the `facefxactor` (`0x1CF649BB`) and `facefxanimationset` (`0x665EF13E`)
//! assets. Each is a UCFX container whose `data` chunk is an OC3 FaceFX SDK archive (`FACE`,
//! version 1610), statically linked into the game.
//!
//! The layout is the game's own deserializer, read in the PS3 v1.00 image (its RTTI names the
//! classes) and its PC v1.0 counterparts:
//!
//! | What | PS3 `ps3-v100` | PC `pc-v10` |
//! |---|---|---|
//! | archive open, header | `0x897b80` | `0x730310` |
//! | name table | `0x897028` | `0x7301e0` → `0x730f20` |
//! | FxString / FxName | `0x8ab2a8` / `0x8abc50` | `0x728c40` / — |
//! | object pointer | `0x89b220` | — |
//! | FxActor | `0x88cd68` | `0x726910` |
//! | FxMasterBoneList / FxBone | `0x8a9238` / `0x8af590` | — / `0x734700` |
//! | FxFaceGraph | `0x89cc70` | `0x72d000` |
//! | FxFaceGraphNode / FxBonePoseNode | `0x89f3b8` / `0x899790` | `0x729ea0` / `0x72c600` |
//! | FxFaceGraphNodeLink / link params | `0x8a06e8` / `0x8a2648` | `0x733ec0` / — |
//! | FxFaceGraphNodeUserProperty | `0x89dec8` | `0x7298f0` |
//! | FxPhonemeMap | `0x8ae4d0` | — |
//! | FxAnimSet / FxAnimGroup | `0x894e18` / `0x8927b0` | `0x72e200` / `0x72b990` |
//! | FxAnim / FxAnimCurve | `0x890460` / `0x8937a8` | `0x72aff0` / `0x7343a0` |
//!
//! Every class writes its base class, then its own u16 version; the stored versions gate the
//! fields that follow, and every gate the deserializer has is followed here. A part the retail
//! archives have never exercised and whose layout the code leaves open (the object a link
//! function's parameters point to, a back-reference to an object already read) is refused with an
//! error naming it, never skipped.
//!
//! What the fields mean beyond their layout is for the evaluator (`mercs2_anim::facefx`).

use std::collections::HashSet;

/// The highest archive version the game's SDK opens (PS3 `0x8aee28` returns it; PC tests `< 0x64b`).
pub const SDK_VERSION: u32 = 1610;

/// Archive version from which the name table exists and object pointers carry a class name.
const NAME_TABLE_VERSION: u32 = 1600;

/// The archive header: the version and the two strings the exporter stamped.
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    pub version: u32,
    pub licensee: String,
    pub project: String,
    /// The u32 after the two strings; the PC save path writes 1000.
    pub licensee_version: u32,
}

/// A bone transform as FaceFX stores it. `rotation` is a quaternion in `w, x, y, z` order: the
/// constructor (PC, inlined in `0x72c730`) puts 1.0 in its first slot and 0 in the other three.
#[derive(Debug, Clone, PartialEq)]
pub struct Bone {
    pub name: String,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

/// One bone of the actor's master bone list: its reference pose, then the two values the list
/// stores beside it (struct `+0x40` and `+0x44`), whose use is the evaluator's to read.
#[derive(Debug, Clone, PartialEq)]
pub struct MasterBone {
    pub bone: Bone,
    pub index: i32,
    /// Present when the list's leading u16 is non-zero.
    pub weight: Option<f32>,
}

/// A face-graph node's link from another node: the input node's name, the link function's name
/// (looked up by name on load), and its parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub input: String,
    pub function: String,
    pub params: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserProperty {
    pub name: String,
    pub a: i32,
    pub b: i32,
    pub value: f32,
    pub strings: Vec<String>,
}

/// What a node's class adds to the shared node fields.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// The bones this pose sets, as full transforms.
    BonePose(Vec<Bone>),
    Delta,
    Combiner,
    CurrentTime,
    GenericTarget,
    MorphTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub class: String,
    pub name: String,
    pub min: f32,
    pub max: f32,
    pub input_operation: i32,
    pub links: Vec<Link>,
    pub properties: Vec<UserProperty>,
    pub kind: NodeKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhonemeMapping {
    pub phoneme: i32,
    pub target: String,
    pub amount: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PhonemeMap {
    pub mappings: Vec<PhonemeMapping>,
    pub targets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Actor {
    pub name: String,
    pub master_bones: Vec<MasterBone>,
    pub nodes: Vec<Node>,
    /// The default group first (actor version ≥ 2), then the others.
    pub groups: Vec<AnimGroup>,
    pub names: Vec<String>,
    pub phoneme_map: Option<PhonemeMap>,
}

/// One curve key: time, value, slope in, slope out — the order the loader copies them into the
/// runtime key (PC `0x734100`, PS3 `0x8a0ac0`) and the evaluator reads them (`0x7341a0`: the slope
/// out of a segment's first key, the slope in of its second).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key(pub [f32; 4]);

impl Key {
    pub fn time(&self) -> f32 {
        self.0[0]
    }
    pub fn value(&self) -> f32 {
        self.0[1]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    pub name: String,
    pub interpolation: i32,
    pub keys: Vec<Key>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Anim {
    pub name: String,
    pub curves: Vec<Curve>,
    /// The two floats at `+0x20` / `+0x24`; not the start and end time (keys run past them).
    pub f20: f32,
    pub f24: f32,
    pub bone_weights: Vec<(String, f32)>,
    pub s38: String,
    pub s44: String,
    pub u50: Option<u32>,
}

impl Anim {
    /// The anim's start and end (`+0x18` / `+0x1c`) as load computes them (PC `0x72b630`, PS3
    /// `0x88f5e0`): from `FLT_MAX` / `FLT_MIN`, the least first-key and the greatest last-key time
    /// over the curves, a curve with no keys counting 0 for both; when neither moved off its
    /// starting value, 0 and 1.
    pub fn bounds(&self) -> (f32, f32) {
        let (mut start, mut end) = (f32::MAX, f32::MIN_POSITIVE);
        for c in &self.curves {
            let (f, l) = match (c.keys.first(), c.keys.last()) {
                (Some(f), Some(l)) => (f.time(), l.time()),
                _ => (0.0, 0.0),
            };
            start = start.min(f);
            end = end.max(l);
        }
        if start == f32::MAX && end == f32::MIN_POSITIVE {
            (0.0, 1.0)
        } else {
            (start, end)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimGroup {
    pub name: String,
    pub anims: Vec<Anim>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimSet {
    pub name: String,
    /// The actor the set was authored against.
    pub actor: String,
    pub group: AnimGroup,
}

/// An archive's header, name table and object-table hint; `T` is its root object.
#[derive(Debug, Clone, PartialEq)]
pub struct Archive<T> {
    pub header: Header,
    pub names: Vec<String>,
    /// The u32 after the name table: the object table's size, which load reserves.
    pub object_hint: u32,
    pub root: T,
}

/// Read a `facefxactor` asset (the UCFX container).
pub fn parse_actor_container(container: &[u8]) -> Result<Archive<Actor>, String> {
    parse_archive(&data_chunk(container)?, read_actor)
}

/// Read a `facefxanimationset` asset (the UCFX container).
pub fn parse_animset_container(container: &[u8]) -> Result<Archive<AnimSet>, String> {
    parse_archive(&data_chunk(container)?, read_animset)
}

fn data_chunk(container: &[u8]) -> Result<Vec<u8>, String> {
    crate::ucfx::extract_chunk_body(container, b"data").ok_or_else(|| "no UCFX `data` chunk".to_string())
}

/// Read a FaceFX archive whose root `read_root` reads; every byte must be read.
pub fn parse_archive<T>(bytes: &[u8], read_root: fn(&mut Reader) -> Result<T, String>) -> Result<Archive<T>, String> {
    let mut r = Reader { b: bytes, pos: 0, version: 0, names: Vec::new(), seen: HashSet::new() };
    let magic = r.take(4)?;
    match magic {
        b"FACE" => {}
        b"FACB" => return Err("a big-endian (`FACB`) archive: only little-endian archives are read".into()),
        _ => return Err(format!("magic {magic:02X?} is not FACE")),
    }
    let version = r.u32()?;
    if version > SDK_VERSION {
        return Err(format!("archive version {version} is newer than the game's SDK ({SDK_VERSION})"));
    }
    r.version = version;
    let header = Header { version, licensee: r.string()?, project: r.string()?, licensee_version: r.u32()? };
    let mut object_hint = 0;
    if version >= NAME_TABLE_VERSION {
        r.u16()?;
        r.u16()?;
        let n = r.u32()?;
        for _ in 0..n {
            r.u16()?;
            let s = r.string()?;
            r.names.push(s);
        }
        object_hint = r.u32()?;
    }
    let root = read_root(&mut r)?;
    if r.pos != bytes.len() {
        return Err(format!("{} bytes left after the root object (read to 0x{:X} of 0x{:X})", bytes.len() - r.pos, r.pos, bytes.len()));
    }
    Ok(Archive { header, names: r.names, object_hint, root })
}

/// The archive cursor: the bytes, the archive version, its name table and the object indices read.
pub struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
    version: u32,
    names: Vec<String>,
    seen: HashSet<u32>,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.b.len()).ok_or_else(|| {
            format!("{n} bytes wanted at 0x{:X}, the archive is 0x{:X} long", self.pos, self.b.len())
        })?;
        let s = &self.b[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn f32s<const N: usize>(&mut self) -> Result<[f32; N], String> {
        let mut a = [0.0; N];
        for v in &mut a {
            *v = self.f32()?;
        }
        Ok(a)
    }
    /// A count, refused when it could not fit in what is left at `min_each` bytes per item.
    fn count(&mut self, min_each: usize, what: &str) -> Result<usize, String> {
        let at = self.pos;
        let n = self.u32()? as usize;
        if n.saturating_mul(min_each) > self.b.len() - self.pos {
            return Err(format!("{what} count {n} at 0x{at:X} overruns the archive"));
        }
        Ok(n)
    }
    /// FxString: u16 version, u32 length, the bytes (no terminator).
    fn string(&mut self) -> Result<String, String> {
        self.u16()?;
        let at = self.pos;
        let n = self.count(1, "string length")?;
        let s = self.take(n)?;
        String::from_utf8(s.to_vec()).map_err(|e| format!("string at 0x{at:X}: {e}"))
    }
    /// FxName: u16 version; 0 → an inline FxString, else a u32 index into the name table.
    fn name(&mut self) -> Result<String, String> {
        if self.u16()? == 0 {
            return self.string();
        }
        let at = self.pos;
        let i = self.u32()? as usize;
        self.names.get(i).cloned().ok_or_else(|| format!("name index {i} at 0x{at:X}: the table holds {}", self.names.len()))
    }
    /// FxObject: its u16 version.
    fn object(&mut self) -> Result<u16, String> {
        self.u16()
    }
    /// FxNamedObject: FxObject, its own u16 version, the name.
    fn named(&mut self) -> Result<String, String> {
        self.object()?;
        self.u16()?;
        self.name()
    }
    /// An object pointer: `None` for null, else the class name of the object that follows.
    fn pointer(&mut self) -> Result<Option<String>, String> {
        let at = self.pos;
        let index = self.u32()?;
        if index == u32::MAX {
            return Ok(None);
        }
        if !self.seen.insert(index) {
            return Err(format!("object pointer at 0x{at:X} refers back to object {index}: back-references are not read"));
        }
        if self.version < NAME_TABLE_VERSION {
            return Err(format!("object pointer at 0x{at:X} in a version {} archive: pre-{NAME_TABLE_VERSION} class encoding is not read", self.version));
        }
        let class = self.name()?;
        // The class's descriptor size; load reads it and does not use it.
        self.u32()?;
        Ok(Some(class))
    }
}

fn read_bone(r: &mut Reader) -> Result<Bone, String> {
    let name = r.named()?;
    r.u16()?;
    Ok(Bone { name, position: r.f32s()?, rotation: r.f32s()?, scale: r.f32s()? })
}

fn read_bones(r: &mut Reader) -> Result<Vec<Bone>, String> {
    r.u16()?;
    let n = r.count(52, "bone")?;
    (0..n).map(|_| read_bone(r)).collect()
}

fn read_master_bones(r: &mut Reader) -> Result<Vec<MasterBone>, String> {
    let weighted = r.u16()? != 0;
    let n = r.count(56, "master bone")?;
    (0..n)
        .map(|_| {
            let bone = read_bone(r)?;
            let index = r.i32()?;
            let weight = if weighted { Some(r.f32()?) } else { None };
            Ok(MasterBone { bone, index, weight })
        })
        .collect()
}

fn read_link(r: &mut Reader) -> Result<Link, String> {
    r.object()?;
    r.u16()?;
    let input = r.name()?;
    let function = r.name()?;
    r.u16()?;
    r.u16()?;
    let n = r.count(4, "link parameter")?;
    let params = (0..n).map(|_| r.f32()).collect::<Result<_, _>>()?;
    if let Some(class) = r.pointer()? {
        return Err(format!("link {input} → {function}: its parameters point to a {class}, whose layout is not read"));
    }
    Ok(Link { input, function, params })
}

fn read_property(r: &mut Reader) -> Result<UserProperty, String> {
    let name = r.named()?;
    r.u16()?;
    let (a, b, value) = (r.i32()?, r.i32()?, r.f32()?);
    r.u16()?;
    let n = r.count(6, "property string")?;
    let strings = (0..n).map(|_| r.string()).collect::<Result<_, _>>()?;
    Ok(UserProperty { name, a, b, value, strings })
}

fn read_node(r: &mut Reader, class: String) -> Result<Node, String> {
    let name = r.named()?;
    let version = r.u16()?;
    let (min, max, input_operation) = (r.f32()?, r.f32()?, r.i32()?);
    r.u16()?;
    let n = r.count(16, "link")?;
    let links = (0..n).map(|_| read_link(r)).collect::<Result<_, _>>()?;
    let mut properties = Vec::new();
    if version >= 1 {
        r.u16()?;
        let n = r.count(28, "user property")?;
        properties = (0..n).map(|_| read_property(r)).collect::<Result<_, _>>()?;
    }
    if version >= 2 {
        // The output list's reserve size.
        r.u32()?;
    }
    let kind = match class.as_str() {
        "FxBonePoseNode" => {
            r.u16()?;
            NodeKind::BonePose(read_bones(r)?)
        }
        "FxDeltaNode" => NodeKind::Delta,
        "FxCombinerNode" => NodeKind::Combiner,
        "FxCurrentTimeNode" => NodeKind::CurrentTime,
        "FxGenericTargetNode" => NodeKind::GenericTarget,
        "FxMorphTargetNode" => {
            // FxGenericTargetNode's version, then its own.
            r.u16()?;
            NodeKind::MorphTarget
        }
        _ => return Err(format!("face-graph node {name:?} is a {class}, which is not read")),
    };
    if !matches!(kind, NodeKind::BonePose(_)) {
        r.u16()?;
    }
    Ok(Node { class, name, min, max, input_operation, links, properties, kind })
}

fn read_face_graph(r: &mut Reader) -> Result<Vec<Node>, String> {
    r.object()?;
    let version = r.u16()?;
    r.u16()?;
    let n = r.count(4, "face-graph node")?;
    let mut nodes = Vec::with_capacity(n);
    for i in 0..n {
        let at = r.pos;
        let class = r.pointer()?.ok_or_else(|| format!("face-graph node {i} at 0x{at:X} is null"))?;
        nodes.push(read_node(r, class)?);
    }
    if version != 0 {
        // Reserve sizes only.
        let n = r.count(4, "face-graph reserve")?;
        for _ in 0..n {
            r.u32()?;
        }
    }
    Ok(nodes)
}

fn read_phoneme_map(r: &mut Reader) -> Result<PhonemeMap, String> {
    r.object()?;
    r.u16()?;
    r.u16()?;
    let n = r.count(14, "phoneme mapping")?;
    let mappings = (0..n)
        .map(|_| Ok(PhonemeMapping { phoneme: r.i32()?, target: r.name()?, amount: r.f32()? }))
        .collect::<Result<_, String>>()?;
    r.u16()?;
    let m = r.count(6, "phoneme target")?;
    let targets = (0..m).map(|_| r.name()).collect::<Result<_, _>>()?;
    Ok(PhonemeMap { mappings, targets })
}

/// The root of a `facefxactor` archive.
pub fn read_actor(r: &mut Reader) -> Result<Actor, String> {
    let name = r.named()?;
    let version = r.u16()?;
    let master_bones = read_master_bones(r)?;
    let nodes = read_face_graph(r)?;
    let mut groups = Vec::new();
    if version >= 2 {
        groups.push(read_anim_group(r)?);
    }
    r.u16()?;
    let n = r.count(16, "anim group")?;
    for _ in 0..n {
        groups.push(read_anim_group(r)?);
    }
    let n = r.count(6, "actor name")?;
    let names = (0..n).map(|_| r.name()).collect::<Result<_, _>>()?;
    let phoneme_map = if version != 0 { Some(read_phoneme_map(r)?) } else { None };
    Ok(Actor { name, master_bones, nodes, groups, names, phoneme_map })
}

fn read_curve(r: &mut Reader) -> Result<Curve, String> {
    let name = r.named()?;
    let version = r.u16()?;
    let interpolation = r.i32()?;
    let mut keys = Vec::new();
    if version == 0 {
        r.u16()?;
        let n = r.count(18, "curve key")?;
        for _ in 0..n {
            r.u16()?;
            keys.push(Key(r.f32s()?));
        }
    }
    Ok(Curve { name, interpolation, keys })
}

fn read_anim(r: &mut Reader) -> Result<Anim, String> {
    let name = r.named()?;
    let version = r.u16()?;
    if version < 4 {
        r.i32()?;
    }
    r.u16()?;
    let n = r.count(16, "curve")?;
    let mut curves = (0..n).map(|_| read_curve(r)).collect::<Result<Vec<_>, _>>()?;
    if version > 5 {
        let total = r.count(16, "anim key")?;
        if total != 0 {
            let keys = (0..total).map(|_| Ok(Key(r.f32s()?))).collect::<Result<Vec<_>, String>>()?;
            r.u16()?;
            let m = r.count(4, "keys-per-curve")?;
            let per: Vec<usize> = (0..m).map(|_| r.u32().map(|v| v as usize)).collect::<Result<_, _>>()?;
            if m != curves.len() || per.iter().sum::<usize>() != total {
                return Err(format!(
                    "anim {name:?}: {total} keys over {m} per-curve counts summing to {} for {} curves",
                    per.iter().sum::<usize>(),
                    curves.len()
                ));
            }
            let mut it = keys.into_iter();
            for (c, k) in curves.iter_mut().zip(per) {
                c.keys.extend(it.by_ref().take(k));
            }
        }
    }
    if version < 5 {
        r.u16()?;
        r.u32()?;
    }
    let (f20, f24) = (r.f32()?, r.f32()?);
    let mut bone_weights = Vec::new();
    if version > 1 {
        r.u16()?;
        let n = r.count(12, "bone weight")?;
        for _ in 0..n {
            r.u16()?;
            bone_weights.push((r.name()?, r.f32()?));
        }
    }
    let (mut s38, mut s44, mut u50) = (String::new(), String::new(), None);
    if version != 0 {
        s38 = r.string()?;
        s44 = r.string()?;
        if version > 2 {
            u50 = Some(r.u32()?);
        }
    }
    Ok(Anim { name, curves, f20, f24, bone_weights, s38, s44, u50 })
}

fn read_anim_group(r: &mut Reader) -> Result<AnimGroup, String> {
    let name = r.named()?;
    r.u16()?;
    r.u16()?;
    let n = r.count(16, "anim")?;
    let anims = (0..n).map(|_| read_anim(r)).collect::<Result<_, _>>()?;
    Ok(AnimGroup { name, anims })
}

/// The root of a `facefxanimationset` archive.
pub fn read_animset(r: &mut Reader) -> Result<AnimSet, String> {
    let name = r.named()?;
    r.u16()?;
    let actor = r.name()?;
    let group = read_anim_group(r)?;
    Ok(AnimSet { name, actor, group })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Little-endian archive writer for the tests: the primitives the reader takes apart.
    #[derive(Default)]
    struct W(Vec<u8>);
    impl W {
        fn u16(&mut self, v: u16) -> &mut Self {
            self.0.extend(v.to_le_bytes());
            self
        }
        fn u32(&mut self, v: u32) -> &mut Self {
            self.0.extend(v.to_le_bytes());
            self
        }
        fn f32(&mut self, v: f32) -> &mut Self {
            self.u32(v.to_bits())
        }
        fn string(&mut self, s: &str) -> &mut Self {
            self.u16(1).u32(s.len() as u32);
            self.0.extend(s.as_bytes());
            self
        }
        fn name(&mut self, i: u32) -> &mut Self {
            self.u16(1).u32(i)
        }
        fn named(&mut self, i: u32) -> &mut Self {
            self.u16(0).u16(0).name(i)
        }
        fn header(&mut self, names: &[&str], hint: u32) -> &mut Self {
            self.0.extend(b"FACE");
            self.u32(SDK_VERSION).string("L").string("P").u32(1000).u16(0).u16(0).u32(names.len() as u32);
            for n in names {
                self.u16(0).string(n);
            }
            self.u32(hint)
        }
    }

    /// An anim set of one anim with two curves whose keys live in the anim's bulk block, handed
    /// out in curve order by the per-curve counts.
    fn animset_bytes() -> Vec<u8> {
        let mut w = W::default();
        w.header(&["Set", "NewActor", "Hello", "Blink", "Jaw"], 0);
        w.named(0).u16(0).name(1);
        w.named(0).u16(0).u16(0).u32(1);
        w.named(2).u16(6).u16(0).u32(2);
        w.named(3).u16(1).u32(0);
        w.named(4).u16(1).u32(0);
        w.u32(3);
        for k in [[0.0, 0.0, 0.0, 0.0], [0.5, 1.0, 0.0, 0.0], [0.25, 0.75, 0.0, 0.0]] {
            for v in k {
                w.f32(v);
            }
        }
        w.u16(0).u32(2).u32(2).u32(1);
        w.f32(0.1).f32(0.2);
        w.u16(0).u32(0);
        w.string("").string("").u32(u32::MAX);
        w.0
    }

    #[test]
    fn an_anim_hands_its_bulk_keys_to_its_curves_in_order() {
        let a = parse_archive(&animset_bytes(), read_animset).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((a.root.name.as_str(), a.root.actor.as_str()), ("Set", "NewActor"));
        let anim = &a.root.group.anims[0];
        assert_eq!(anim.name, "Hello");
        let keys: Vec<(&str, Vec<f32>)> =
            anim.curves.iter().map(|c| (c.name.as_str(), c.keys.iter().map(Key::value).collect())).collect();
        assert_eq!(keys, vec![("Blink", vec![0.0, 1.0]), ("Jaw", vec![0.75])]);
        assert_eq!(anim.bounds(), (0.0, 0.5));
    }

    #[test]
    fn a_byte_left_over_or_missing_is_refused() {
        let mut b = animset_bytes();
        b.push(0);
        assert!(parse_archive(&b, read_animset).unwrap_err().contains("1 bytes left"));
        b.truncate(b.len() - 2);
        assert!(parse_archive(&b, read_animset).is_err());
    }

    #[test]
    fn per_curve_counts_that_do_not_cover_the_keys_are_refused() {
        let mut b = animset_bytes();
        // The second per-curve count (1 → 2): the counts now sum to 4 for 3 keys. After it come
        // the two floats (8), the bone-weight count (6), two empty strings (12) and a u32 (4).
        let at = b.len() - 30 - 4;
        b[at..at + 4].copy_from_slice(&2u32.to_le_bytes());
        assert!(parse_archive(&b, read_animset).unwrap_err().contains("per-curve counts"));
    }

    #[test]
    fn a_node_class_with_no_reader_is_refused() {
        let mut w = W::default();
        w.header(&["A", "FxMysteryNode", "N"], 1);
        w.named(0).u16(2);
        w.u16(1).u32(0);
        w.u16(0).u16(1).u16(0).u32(1);
        w.u32(0).name(1).u32(92);
        w.named(2).u16(2).f32(0.0).f32(1.0).u32(0).u16(0).u32(0).u16(0).u32(0).u32(0);
        assert!(parse_archive(&w.0, read_actor).unwrap_err().contains("FxMysteryNode, which is not read"));
    }
}
