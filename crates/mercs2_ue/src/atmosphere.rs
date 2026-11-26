//! The game's lighting: the `AtmosphereBase` settings a level ships, and the light they give.
//!
//! The game's world scripts do not set the Venezuela level's look on PC (`Graphics.Atmosphere.SetSky`
//! is a stub there). The look is data: `AtmosphereBase` component records, each a named setting
//! (`default`, `warzone`, `night`, …) on an entity. An entity with a `LineRegion` is a region of the
//! map; the game's tick (`FUN_006A4480`) tests the viewer against every region's polygon and blends
//! the live atmosphere toward the setting of the region it is in, or toward the `default` record of
//! entity `0x900000CF` when it is in none. Every frame the game builds a light block from the live
//! atmosphere (`FUN_00465630`): minus the direction toward the sun, the light's colour, the rim
//! colour, and six ambient faces.
//!
//! [`Settings`] reads the records and the region polygons; [`Setting::atmosphere`] gives a record's
//! light block. `data/atmosphere_captures.tsv` holds that block as read from retail memory captures,
//! which the retail test holds the records against.
//!
//! The record layout, the region test and the blend are read in the notes repo's
//! [`docs/reverse_engineer/atmosphere/README.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/atmosphere/README.md).

use crate::data::{self, Game, LAYER_STATIC};
use crate::space;
use mercs2_formats::schema::{parse_comp_groups, FieldValue};
use std::ffi::CString;
use std::sync::OnceLock;

/// The lighting of one setting or one captured moment, directions in Unreal's axes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    /// The game's time of day, 0..1 across the day (0.5 has the sun overhead).
    pub time: f32,
    /// The unit direction toward the sun. The game's directional light travels the other way.
    pub toward_sun: [f32; 3],
    /// The directional light's colour, linear, light intensity folded in: a surface facing the sun
    /// takes its diffuse colour times this.
    pub light_color: [f32; 3],
    pub rim_color: [f32; 3],
    /// The six ambient faces, in the game's order: each the ambient cube's face times the ambient
    /// colour. Which direction each face lights is not read from the game's shaders.
    pub ambient: [[f32; 3]; 6],
    /// The Henyey-Greenstein constant of the sky's Mie phase function (`fScatterHGg`).
    pub mie_g: f32,
    /// The entity whose record this is: a region, or [`GLOBAL_ENTITY`]. 0 for a captured moment.
    pub entity: u32,
}

/// The component class of an atmosphere setting.
const CLASS: &str = "AtmosphereBase";
/// The `lineregion` asset type (`mercs2_formats::types`).
const LINEREGION_TYPE_HASH: u32 = 0x6310_807F;
/// The entity whose `default` record the game applies when the viewer is in no region: the tick's
/// `Change(0x900000CF, "default")`. It is in the `worldentity`, named `Atmosphere`.
pub const GLOBAL_ENTITY: u32 = 0x9000_00CF;
/// The live atmosphere's `LightAngle` (`+0x18`), in degrees. No record holds it: only
/// `Graphics.Atmosphere.SetLightAngle` writes it, and it reads -150 in all five retail captures read.
pub const LIGHT_ANGLE_DEGREES: f32 = -150.0;

/// One `AtmosphereBase` record: a named setting of an entity.
#[derive(Debug, Clone)]
pub struct Setting {
    pub entity: u32,
    /// `pandemic_hash_m2` of the setting's name (the record's `Name` field).
    pub name: u32,
    fields: Vec<(u32, FieldValue)>,
}

impl Setting {
    fn field(&self, name: &str) -> Result<FieldValue, String> {
        let hash = data::hash(name);
        self.fields.iter().find(|(h, _)| *h == hash).map(|(_, v)| *v).ok_or_else(|| format!("{CLASS} has no field {name}"))
    }

    fn float(&self, name: &str) -> Result<f32, String> {
        match self.field(name)? {
            FieldValue::F32(v) => Ok(v),
            v => Err(format!("{CLASS}.{name} is {v:?}, not a float")),
        }
    }

    /// A colour field times its multiplier field: a packed RGBA word, red in the low byte, each
    /// byte / 255 (the record reader `FUN_004ed6f0`).
    fn color(&self, name: &str) -> Result<[f32; 3], String> {
        let word = match self.field(name)? {
            FieldValue::U32(v) => v,
            v => return Err(format!("{CLASS}.{name} is {v:?}, not a packed colour")),
        };
        let scale = self.float(&format!("{name}Multiplier"))? / 255.0;
        let bytes = word.to_le_bytes();
        Ok([f32::from(bytes[0]) * scale, f32::from(bytes[1]) * scale, f32::from(bytes[2]) * scale])
    }

    /// The light block of this setting once the live atmosphere has settled on it.
    ///
    /// The ambient faces and the rim colour are the record's own values, as the block holds them.
    /// Two fields are derived, and their producer in the exe is not found:
    ///
    /// - the sun's direction, from `LightTime` and [`LIGHT_ANGLE_DEGREES`]: with θ = π · time and
    ///   A the angle, toward the sun is `(-cos θ · cos A, sin θ, -cos θ · sin A)` in game axes. A
    ///   fit that holds in five captures to 3e-4;
    /// - the light's colour, as `LightMultiplier × LightIntensity`. The game's is that times a tint
    ///   that reddens as the sun gets lower: in the captures it is 1.03 to 1.10 of this in red and
    ///   0.82 to 0.94 in blue.
    pub fn atmosphere(&self) -> Result<Atmosphere, String> {
        let time = self.float("LightTime")?;
        let (theta, angle) = (std::f32::consts::PI * time, LIGHT_ANGLE_DEGREES.to_radians());
        let toward_sun = [-theta.cos() * angle.cos(), theta.sin(), -theta.cos() * angle.sin()];
        let intensity = self.float("LightIntensity")?;
        let light_color = [
            self.float("LightMultiplier_X")? * intensity,
            self.float("LightMultiplier_Y")? * intensity,
            self.float("LightMultiplier_Z")? * intensity,
        ];
        let ambient_color = self.color("AmbientColor")?;
        let mut ambient = [[0.0f32; 3]; 6];
        for (face, out) in ambient.iter_mut().enumerate() {
            let cube = self.color(&format!("AmbientCube{face}"))?;
            *out = std::array::from_fn(|c| cube[c] * ambient_color[c]);
        }
        Ok(Atmosphere {
            time,
            toward_sun: space::direction(toward_sun),
            light_color,
            rim_color: self.color("RimColor")?,
            ambient,
            mie_g: self.float("ScatterHGg")?,
            entity: self.entity,
        })
    }
}

/// A region of the map: an entity with settings and a `LineRegion`, its polygon in the game's X and Z
/// (metres).
#[derive(Debug, Clone)]
pub struct Region {
    pub entity: u32,
    pub polygon: Vec<[f32; 2]>,
}

impl Region {
    /// Whether the region holds a point, by the game's test (`FUN_0045EBB0`): an even-odd count of
    /// the polygon's edges a ray along +X from the point crosses. There is no border width.
    pub fn contains(&self, x: f32, z: f32) -> bool {
        let n = self.polygon.len();
        let mut inside = false;
        for i in 0..n {
            let (a, b) = (self.polygon[i], self.polygon[(i + 1) % n]);
            if (a[1] > z) != (b[1] > z) && x < a[0] + (z - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
                inside = !inside;
            }
        }
        inside
    }
}

/// Every atmosphere setting of the static world, and its regions.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// In the records' order: the layers ascending by name hash, then the `worldentity`'s.
    pub settings: Vec<Setting>,
    /// The entities with a setting and a region polygon, in the order of their first setting.
    pub regions: Vec<Region>,
}

/// A `lineregion` asset's points: `u16 kind, u16 n, u32 0, n × (x, z)`, local to its entity. The
/// game's region test takes kinds 2 and 4; any other kind is no region.
fn region_points(body: &[u8]) -> Result<Option<Vec<[f32; 2]>>, String> {
    if body.len() < 8 {
        return Err(format!("a lineregion of {} bytes", body.len()));
    }
    let (kind, n) = (u16::from_le_bytes([body[0], body[1]]), usize::from(u16::from_le_bytes([body[2], body[3]])));
    if body.len() < 8 + n * 8 {
        return Err(format!("a lineregion of {n} points in {} bytes", body.len()));
    }
    if kind != 2 && kind != 4 {
        return Ok(None);
    }
    let f = |at: usize| f32::from_le_bytes([body[at], body[at + 1], body[at + 2], body[at + 3]]);
    Ok(Some((0..n).map(|i| [f(8 + i * 8), f(12 + i * 8)]).collect()))
}

impl Settings {
    /// Read the settings the layers of the static world place and the `worldentity` carries. A
    /// layer a save loads on top of the static world is not read.
    pub fn read(game: &mut Game) -> Result<Settings, String> {
        use mercs2_formats::placement::{component_words, placed_entities};
        use mercs2_formats::worldentity::{Payload, WorldEntity, RETAIL_WORLDENTITY_NAME_HASH, WORLDENTITY_TYPE_HASH};
        let name_field = data::hash("Name");
        let mut out = Settings::default();
        let setting = |entity: u32, fields: Vec<(u32, FieldValue)>| -> Result<Setting, String> {
            match fields.iter().find(|(h, _)| *h == name_field) {
                Some((_, FieldValue::U32(name))) => Ok(Setting { entity, name: *name, fields }),
                other => Err(format!("entity 0x{entity:08X}: its {CLASS} record's Name is {other:?}")),
            }
        };
        for layer in game.layers()?.into_iter().filter(|l| l.info.kind == LAYER_STATIC) {
            let what = |e: String| format!("layer 0x{:08X}: {e}", layer.info.hash);
            let container = game
                .assets
                .ecs_container(mercs2_engine::worldutil::LAYER_TYPE_HASH, layer.info.hash)
                .map_err(what)?
                .ok_or_else(|| what("in no open wad".into()))?;
            let first = out.settings.len();
            for c in mercs2_formats::ucfx::placement_containers(&container, "layer").0 {
                for group in parse_comp_groups(&c).into_iter().filter(|g| g.name.as_deref() == Some(CLASS)) {
                    let schema = group.schema().ok_or_else(|| what(format!("{CLASS} has no schema")))?;
                    let records = group.data.as_deref().unwrap_or(&[]);
                    let records = schema.deserialize_records(records).ok_or_else(|| what(format!("{CLASS} data of {} bytes is not whole records", records.len())))?;
                    for r in records {
                        out.settings.push(setting(r.entity_key, r.fields).map_err(what)?);
                    }
                }
            }
            if out.settings.len() == first {
                continue;
            }
            // The regions of this layer's settings: each entity's `LineRegion` polygon, placed at
            // the entity's X and Z with no rotation (`FUN_0045E970`).
            let places: std::collections::HashMap<u32, [f32; 3]> = placed_entities(&container).map_err(what)?.into_iter().map(|e| (e.key, e.pos)).collect();
            let assets: std::collections::HashMap<u32, u32> = component_words(&container, "LineRegion").map_err(what)?.into_iter().collect();
            for at in first..out.settings.len() {
                let entity = out.settings[at].entity;
                if out.regions.iter().any(|r| r.entity == entity) {
                    continue;
                }
                let Some(&asset) = assets.get(&entity) else { continue };
                let place = places.get(&entity).ok_or_else(|| what(format!("region 0x{entity:08X} has no Transform")))?;
                let body = mercs2_engine::wad::extract_container_typed(game.assets.base_mut(), asset, LINEREGION_TYPE_HASH)
                    .map_err(|e| what(format!("lineregion 0x{asset:08X}: {e}")))?;
                let body = mercs2_formats::ucfx::extract_chunk_body(&body, b"data").ok_or_else(|| what(format!("lineregion 0x{asset:08X}: no data chunk")))?;
                if let Some(points) = region_points(&body).map_err(|e| what(format!("lineregion 0x{asset:08X}: {e}")))? {
                    out.regions.push(Region { entity, polygon: points.into_iter().map(|[x, z]| [place[0] + x, place[2] + z]).collect() });
                }
            }
        }
        let we = game
            .assets
            .ecs_container(WORLDENTITY_TYPE_HASH, RETAIL_WORLDENTITY_NAME_HASH)?
            .ok_or("worldentity 0x50075B3B: in no open wad")?;
        let we = WorldEntity::parse(&we).map_err(|e| format!("worldentity: {e}"))?;
        for group in we.groups_of(CLASS) {
            for r in &group.records {
                let Payload::Fixed(bytes) = &r.payload else {
                    return Err(format!("worldentity: a {CLASS} record of keys {:08X?} is not fixed-size", r.keys));
                };
                for &entity in &r.keys {
                    out.settings.push(setting(entity, group.schema.read_payload(bytes))?);
                }
            }
        }
        if !out.settings.iter().any(|s| s.entity == GLOBAL_ENTITY) {
            return Err(format!("no {CLASS} record of the global entity 0x{GLOBAL_ENTITY:08X}"));
        }
        Ok(out)
    }

    /// The entity whose setting applies at a point of the game's X and Z: the last region, in the
    /// regions' order, that holds it, or [`GLOBAL_ENTITY`] when none does.
    ///
    /// Where regions overlap, the one the game has current depends on the path the viewer took: it
    /// changes setting on entering a region, and on leaving one re-applies every region still
    /// around the viewer in table order, so the last wins. This gives that last one.
    pub fn entity_at(&self, x: f32, z: f32) -> u32 {
        self.regions.iter().rev().find(|r| r.contains(x, z)).map_or(GLOBAL_ENTITY, |r| r.entity)
    }

    /// The setting named `name` of the entity that applies at a point ([`Self::entity_at`]). A
    /// region's setting is `default` until a script changes it
    /// (`Graphics.Atmosphere.ChangeLineRegionSetting`).
    pub fn at(&self, x: f32, z: f32, name: &str) -> Result<&Setting, String> {
        let (entity, hash) = (self.entity_at(x, z), data::hash(name));
        self.settings.iter().find(|s| s.entity == entity && s.name == hash).ok_or_else(|| {
            let has: Vec<String> = self.settings.iter().filter(|s| s.entity == entity).map(|s| format!("0x{:08X}", s.name)).collect();
            format!("the atmosphere entity 0x{entity:08X} at ({x}, {z}) has no setting \"{name}\" (0x{hash:08X}); it has {}", has.join(", "))
        })
    }
}

/// One row of the table.
#[derive(Debug)]
pub struct Capture {
    pub name: CString,
    pub atmosphere: Atmosphere,
}

const TABLE: &str = include_str!("../data/atmosphere_captures.tsv");

/// The table's column names, in order.
const COLUMNS: &[&str] = &[
    "name", "time", "sun_x", "sun_y", "sun_z", "light_r", "light_g", "light_b", "rim_r", "rim_g", "rim_b", "face0_r", "face0_g", "face0_b", "face1_r",
    "face1_g", "face1_b", "face2_r", "face2_g", "face2_b", "face3_r", "face3_g", "face3_b", "face4_r", "face4_g", "face4_b", "face5_r", "face5_g",
    "face5_b", "mie_g",
];

fn parse(table: &str) -> Result<Vec<Capture>, String> {
    let mut lines = table.lines().filter(|l| !l.is_empty() && !l.starts_with('#'));
    let header: Vec<&str> = lines.next().ok_or("the atmosphere table has no header")?.split('\t').collect();
    if header != COLUMNS {
        return Err(format!("the atmosphere table's columns are {header:?}"));
    }
    let mut out: Vec<Capture> = Vec::new();
    for line in lines {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != COLUMNS.len() {
            return Err(format!("atmosphere row {:?}: {} fields for {} columns", fields[0], fields.len(), COLUMNS.len()));
        }
        let name = fields[0];
        let n = fields[1..]
            .iter()
            .zip(&COLUMNS[1..])
            .map(|(f, c)| f.parse::<f32>().map_err(|e| format!("atmosphere row {name:?}: {c} {f:?}: {e}")))
            .collect::<Result<Vec<f32>, String>>()?;
        let v3 = |at: usize| [n[at], n[at + 1], n[at + 2]];
        let sun = v3(1);
        let length = sun.iter().map(|c| c * c).sum::<f32>().sqrt();
        if (length - 1.0).abs() > 1e-4 {
            return Err(format!("atmosphere row {name:?}: the sun direction has length {length}"));
        }
        if out.iter().any(|c| c.name.to_bytes() == name.as_bytes()) {
            return Err(format!("atmosphere row {name:?} is in the table twice"));
        }
        out.push(Capture {
            name: CString::new(name).map_err(|e| format!("atmosphere row {name:?}: {e}"))?,
            atmosphere: Atmosphere {
                time: n[0],
                toward_sun: space::direction(sun),
                light_color: v3(4),
                rim_color: v3(7),
                ambient: std::array::from_fn(|face| v3(10 + 3 * face)),
                mie_g: n[28],
                entity: 0,
            },
        });
    }
    if out.is_empty() {
        return Err("the atmosphere table has no row".into());
    }
    Ok(out)
}

impl Game {
    /// The light of the setting named `setting` that applies at a point of the game's space
    /// (metres; the height is not used): [`Settings::at`], then [`Setting::atmosphere`]. The
    /// settings are read once per game.
    pub fn atmosphere_at(&mut self, game_position: [f32; 3], setting: &str) -> Result<Atmosphere, String> {
        if self.atmosphere.is_none() {
            self.atmosphere = Some(Settings::read(self)?);
        }
        let settings = self.atmosphere.as_ref().expect("read above");
        settings.at(game_position[0], game_position[2], setting)?.atmosphere()
    }
}

/// Every captured moment, in the table's order.
pub fn captures() -> Result<&'static [Capture], String> {
    static CAPTURES: OnceLock<Result<Vec<Capture>, String>> = OnceLock::new();
    CAPTURES.get_or_init(|| parse(TABLE)).as_deref().map_err(Clone::clone)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_reads_and_its_suns_are_above_the_horizon() {
        let all = captures().unwrap();
        assert_eq!(all.len(), 4);
        for c in all {
            let a = &c.atmosphere;
            // Unreal's up is Z; every capture is of daytime.
            assert!(a.toward_sun[2] > 0.0, "{:?}", c.name);
            assert!(a.light_color.iter().chain(a.ambient.iter().flatten()).all(|v| *v > 0.0), "{:?}", c.name);
            assert!((0.0..=1.0).contains(&a.time) && (-1.0..1.0).contains(&a.mie_g), "{:?}", c.name);
        }
        // Mid-mission (time 0.2): toward the sun is game (0.70058, 0.58793, 0.40439), Y up.
        let mid = &all[1];
        assert_eq!(mid.name.to_str().unwrap(), "time_0.20");
        assert_eq!(mid.atmosphere.toward_sun, [0.70058, 0.40439, 0.58793]);
        // The sun's height follows the time: sin(pi x time), read off the captures.
        for c in all {
            let want = (std::f32::consts::PI * c.atmosphere.time).sin();
            assert!((c.atmosphere.toward_sun[2] - want).abs() < 2e-3, "{:?}: {} vs {want}", c.name, c.atmosphere.toward_sun[2]);
        }
    }

    /// A record of the fields [`Setting::atmosphere`] reads, with every colour 0x80FF8040 (red 0x40,
    /// green 0x80, blue 0xFF) and every multiplier 2.
    fn record(time: f32) -> Setting {
        let mut fields = vec![(data::hash("Name"), FieldValue::U32(data::hash("default")))];
        let mut float = |name: &str, v: f32| fields.push((data::hash(name), FieldValue::F32(v)));
        float("LightTime", time);
        float("LightIntensity", 1.5);
        float("LightMultiplier_X", 1.0);
        float("LightMultiplier_Y", 0.5);
        float("LightMultiplier_Z", 0.25);
        float("ScatterHGg", 0.7);
        let colors = ["AmbientColor", "AmbientCube0", "AmbientCube1", "AmbientCube2", "AmbientCube3", "AmbientCube4", "AmbientCube5", "RimColor"];
        for name in colors {
            fields.push((data::hash(name), FieldValue::U32(0x80FF_8040)));
            fields.push((data::hash(&format!("{name}Multiplier")), FieldValue::F32(2.0)));
        }
        Setting { entity: 7, name: data::hash("default"), fields }
    }

    #[test]
    fn a_setting_gives_its_light_block() {
        let a = record(0.5).atmosphere().unwrap();
        // Time 0.5: the sun overhead, which is Unreal's +Z.
        assert!(a.toward_sun[0].abs() < 1e-6 && a.toward_sun[1].abs() < 1e-6 && (a.toward_sun[2] - 1.0).abs() < 1e-6, "{:?}", a.toward_sun);
        assert_eq!(a.light_color, [1.5, 0.75, 0.375]);
        let color = [64.0 / 255.0 * 2.0, 128.0 / 255.0 * 2.0, 2.0f32];
        assert_eq!(a.rim_color, color);
        // A face is its cube colour times the ambient colour.
        assert_eq!(a.ambient, [[color[0] * color[0], color[1] * color[1], color[2] * color[2]]; 6]);
        assert_eq!((a.time, a.mie_g, a.entity), (0.5, 0.7, 7));
        // Time 0.2 with the angle -150 degrees: the capture's direction, game (0.70058, 0.58793, 0.40439).
        let low = record(0.2).atmosphere().unwrap();
        for (got, want) in low.toward_sun.iter().zip([0.70058f32, 0.40439, 0.58793]) {
            assert!((got - want).abs() < 3e-4, "{:?}", low.toward_sun);
        }
        let mut cut = record(0.5);
        cut.fields.retain(|(h, _)| *h != data::hash("RimColorMultiplier"));
        assert!(cut.atmosphere().unwrap_err().contains("RimColorMultiplier"));
    }

    #[test]
    fn a_region_holds_the_points_inside_its_polygon() {
        // An L: the square (0,0)..(4,4) without its (2,2)..(4,4) corner.
        let l = Region { entity: 1, polygon: vec![[0.0, 0.0], [4.0, 0.0], [4.0, 2.0], [2.0, 2.0], [2.0, 4.0], [0.0, 4.0]] };
        assert!(l.contains(1.0, 1.0) && l.contains(3.0, 1.0) && l.contains(1.0, 3.0));
        assert!(!l.contains(3.0, 3.0) && !l.contains(-1.0, 1.0) && !l.contains(1.0, 5.0));
        let square = Region { entity: 2, polygon: vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]] };
        let default = |entity: u32| Setting { entity, ..record(0.5) };
        let settings = Settings { settings: vec![default(1), default(2), default(GLOBAL_ENTITY)], regions: vec![l, square] };
        // In both: the later region. In the L only. In neither: the global entity.
        assert_eq!(settings.entity_at(1.0, 1.0), 2);
        assert_eq!(settings.entity_at(1.0, 3.0), 1);
        assert_eq!(settings.entity_at(9.0, 9.0), GLOBAL_ENTITY);
        assert_eq!(settings.at(9.0, 9.0, "default").unwrap().entity, GLOBAL_ENTITY);
        let e = settings.at(1.0, 3.0, "night").unwrap_err();
        assert!(e.contains("0x00000001") && e.contains("\"night\"") && e.contains(&format!("0x{:08X}", data::hash("default"))), "{e}");
        assert_eq!(region_points(&[2, 0, 1, 0, 0, 0, 0, 0, 0, 0, 128, 63, 0, 0, 0, 64]).unwrap(), Some(vec![[1.0, 2.0]]));
        assert_eq!(region_points(&[3, 0, 0, 0, 0, 0, 0, 0]).unwrap(), None);
        assert!(region_points(&[2, 0, 1, 0, 0, 0, 0, 0]).unwrap_err().contains("1 points"));
    }

    #[test]
    fn a_malformed_table_is_an_error() {
        assert!(parse("").unwrap_err().contains("no header"));
        assert!(parse("name\ttime\n").unwrap_err().contains("columns"));
        let header = COLUMNS.join("\t");
        assert!(parse(&format!("{header}\n")).unwrap_err().contains("no row"));
        assert!(parse(&format!("{header}\nx\t0.5\n")).unwrap_err().contains("fields"));
        let row = |sun: &str| format!("{header}\nx\t0.5\t{sun}{}\n", "\t0.5".repeat(25));
        assert!(parse(&row("1\t1\t0")).unwrap_err().contains("length"));
        assert!(parse(&row("0\t1\tq")).unwrap_err().contains("sun_z"));
        let good = row("0\t1\t0");
        assert_eq!(parse(&good).unwrap().len(), 1);
        let twice = format!("{good}x\t0.5\t0\t1\t0{}\n", "\t0.5".repeat(25));
        assert!(parse(&twice).unwrap_err().contains("twice"));
    }
}
