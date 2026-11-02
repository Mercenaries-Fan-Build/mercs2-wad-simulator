//! The Live page's link to the running game: the one owner of the lua-bridge connection.
//!
//! The bridge serves one raw client at a time and runs every chunk on the game thread, inside a frame,
//! with nothing able to abort it. So one worker thread holds the connection, asks in tiers (the pose
//! often, entities less, world state rarely), and waits for each reply before the next ask, so a slow
//! game slows the poll and never queues. The rail dot and the page read one state.
//!
//! Every chunk is a constant here and only reads: `read_only_chunks` below fails on any call off
//! [`ALLOWED`]. A guid is never carried from one chunk to the next, because a stale guid in a native read
//! can kill the game; the selection is held as text and found again in the enumeration. Each chunk
//! returns one string ending in [`END`]: the bridge cuts a result at 16 KiB with no marker, so a reply
//! without it is an error, never short data.
//!
//! Replies are recorded and replayed verbatim (`.replay`, NDJSON), so a recording drives this module the
//! way the game does.

use mercs2_bridge::reply::{parse_reply, Reply, Value};
use mercs2_bridge::{Bridge, BridgeError};
use mercs2_formats::live_reads::STATUS_RESIDENT;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The last field of every reply.
const END: &str = "$";

/// How long a session is held before it is closed and reopened, so a waiting REPL client gets its turn.
/// A session per ask would write two `lua_bridge.log` lines each time (`repl.rs:145,149`).
const SESSION: Duration = Duration::from_secs(5);
/// How long one exchange may take before the game counts as not answering.
const ANSWER: Duration = Duration::from_secs(2);
/// The floor between two sends, the bridge's own default.
const MIN_SEND: Duration = Duration::from_millis(60);
/// The wait between connection attempts.
const RETRY: Duration = Duration::from_secs(1);
/// The reachability poll while the page is closed.
const IDLE_POLL: Duration = Duration::from_secs(3);
/// How long one reachability check waits for the game to accept.
const IDLE_CONNECT: Duration = Duration::from_millis(300);

/// The polling tiers, most frequent first. The order is the priority when several are due.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Pose,
    Entities,
    Mission,
    World,
    Stock,
    Assets,
    Statics,
}

impl Tier {
    const ALL: [Tier; 7] = [Tier::Statics, Tier::Pose, Tier::Entities, Tier::World, Tier::Stock, Tier::Mission, Tier::Assets];

    /// How often this tier is asked; `None` = once per session of the page.
    fn period(self) -> Option<Duration> {
        match self {
            Tier::Pose => Some(Duration::from_millis(100)),
            Tier::Entities => Some(Duration::from_millis(500)),
            Tier::Mission => Some(Duration::from_secs(1)),
            Tier::World | Tier::Stock | Tier::Assets => Some(Duration::from_secs(2)),
            Tier::Statics => None,
        }
    }

    /// The fixed chunks. [`Tier::Assets`] is built from the assets asked about: [`assets_chunk`].
    fn chunk(self) -> &'static str {
        match self {
            Tier::Pose => POSE_CHUNK,
            Tier::Entities => ENTITIES_CHUNK,
            Tier::Mission => MISSION_CHUNK,
            Tier::World => WORLD_CHUNK,
            Tier::Stock => STOCK_CHUNK,
            Tier::Statics => STATICS_CHUNK,
            Tier::Assets => unreachable!("the assets chunk is built per slice"),
        }
    }
}

/// Which tier is due at `now`, oldest deadline first among those due.
#[derive(Debug, Default)]
struct Schedule {
    next: Vec<(Tier, Instant)>,
    statics_done: bool,
}

impl Schedule {
    fn new(now: Instant) -> Schedule {
        Schedule { next: Tier::ALL.iter().map(|&t| (t, now)).collect(), statics_done: false }
    }

    fn due(&self, now: Instant) -> Option<Tier> {
        self.next
            .iter()
            .filter(|(t, at)| *at <= now && (*t != Tier::Statics || !self.statics_done))
            .min_by_key(|(_, at)| *at)
            .map(|(t, _)| *t)
    }

    /// `tier` was asked at `now`; a tier late by more than its period does not pile up.
    fn asked(&mut self, tier: Tier, now: Instant) {
        if tier == Tier::Statics {
            self.statics_done = true;
        }
        if let (Some(p), Some(slot)) = (tier.period(), self.next.iter_mut().find(|(t, _)| *t == tier)) {
            slot.1 = now + p;
        }
    }

    fn until_next(&self, now: Instant) -> Duration {
        self.next
            .iter()
            .filter(|(t, _)| *t != Tier::Statics || !self.statics_done)
            .map(|(_, at)| at.saturating_duration_since(now))
            .min()
            .unwrap_or(Duration::from_millis(100))
    }
}

/// `noworld` is the same answer from every chunk: the Lua state has no game in it yet.
macro_rules! guard {
    () => {
        r#"local M=_MODULES
if M==nil or M.mrxplaystate==nil then
  local n,s=-1,"none"
  if Loader then n,s=Loader.GetLoadPhase() end
  return string.format("noworld|%d|%s|$",n,s)
end
"#
    };
}

/// The hero's pose and the camera's angles.
pub const POSE_CHUNK: &str = concat!(
    "-- live:pose\n",
    guard!(),
    r#"local c=Player.GetLocalCharacter()
if c==nil then return "nohero|$" end
local p=Player.GetLocalPlayer()
local x,y,z=Object.GetPosition(c)
local yaw=Object.GetYaw(c)
local hp,mx=Object.GetHealth(c),Object.GetMaxHealth(c)
local v="-"
local u=Vehicle.GetFromRider(c)
if u then
  local a,b,d=Object.GetPosition(u)
  v=string.format("%s %.3f %.3f %.3f %.3f",Sys.GuidToString(u),a,b,d,Object.GetYaw(u))
end
local cam="-"
local eye="-"
local k=Player.GetCamera(p)
if k then
  cam=string.format("%.3f %.3f %.3f",Camera.GetYaw(k),Camera.GetPitch(k),Camera.GetFOV(k))
  if M2Live then eye=string.format("%.3f %.3f %.3f",M2Live.CameraEye(k)) end
end
return string.format("pose|%.3f %.3f %.3f %.3f %.1f %.1f|%s|%s|%s|$",x,y,z,yaw,hp,mx,v,cam,eye)
"#
);

/// The hero's identity, then the humans and vehicles nearest it.
pub const ENTITIES_CHUNK: &str = concat!(
    "-- live:entities\n",
    guard!(),
    r#"local c=Player.GetLocalCharacter()
if c==nil then return "nohero|$" end
local x,y,z=Object.GetPosition(c)
local function id(g)
  local f=Ai.GetFactionGuid(g)
  local fs="0x00000000"
  if f then fs=Sys.GuidToString(Object.GetName(f)) end
  return Sys.GuidToString(g),Sys.GuidToString(Object.GetName(Object.GetParent(g))),Sys.GuidToString(Object.GetModelName(g)),fs,Object.GetLocalizedName(g)
end
local function row(g,cls)
  local a,b,d=Object.GetPosition(g)
  local s,t,m,f,n=id(g)
  return string.format("%s %s %.2f %.2f %.2f %.1f %.1f %.1f %s %s %s %s",s,cls,a,b,d,Object.GetYaw(g),Object.GetHealth(g),Object.GetMaxHealth(g),t,m,f,n)
end
local all={}
local me=Sys.GuidToString(c)
local function add(l,cls)
  for _,g in ipairs(l) do
    if Sys.GuidToString(g)~=me then
      local a,b,d=Object.GetPosition(g)
      local dx,dy,dz=a-x,b-y,d-z
      all[#all+1]={dx*dx+dy*dy+dz*dz,g,cls}
    end
  end
end
add(Pg.FastCollectHumans(x,y,z,250),"human")
add(Pg.FastCollectCars(x,y,z,250),"car")
add(Pg.FastCollectTanks(x,y,z,250),"tank")
add(Pg.FastCollectHelicopters(x,y,z,250),"helicopter")
add(Pg.FastCollectJets(x,y,z,250),"jet")
add(Pg.FastCollectBoats(x,y,z,250),"boat")
table.sort(all,function(p,q) return p[1]<q[1] end)
local t={row(c,"hero")}
for i=1,math.min(#all,127) do t[#t+1]=row(all[i][2],all[i][3]) end
return "entities|"..table.concat(t,";").."|$"
"#
);

/// The loaded dynamic layers and the level's state.
pub const WORLD_CHUNK: &str = concat!(
    "-- live:world\n",
    guard!(),
    r#"local l={}
local gone=0
for k,v in pairs(M.mrxlayermanager._tLoadedLayers) do
  if v then gone=gone+1 else l[#l+1]=k end
end
local f=M.mrxmissionflow
local jobs=0
for _,v in pairs(f._tActiveMissions) do
  if v.oMission and v.oMission.IsJob then jobs=jobs+1 end
end
local host="reimpl"
if Loader then host="retail" end
local c=Player.GetLocalCharacter()
local hero="-"
if c then
  local b=Object.GetParent(c)
  if Object.HasLabel(b,"Mattias") then hero="Mattias" elseif Object.HasLabel(b,"Jennifer") then hero="Jennifer" elseif Object.HasLabel(b,"Chris") then hero="Chris" end
end
local s="0"
if Sys.IsLoadingOrStreaming() then s="1" end
local pn,ps=-1,"none"
if Loader then pn,ps=Loader.GetLoadPhase() end
local rs="-"
if M2Live then rs=string.format("%d %d",M2Live.Residency()) end
return string.format("world|%s|%s|%s|%s|%d|%d|%s|%d|%d|%d|%s|%s|%s|$",host,Sys.GetLevelName(),hero,tostring(f.GetLastCompletedContractName()),M.mrxplaystate.Get(),f._nBlockingSequences or 0,s,jobs,gone,pn,ps,rs,table.concat(l,","))
"#
);

/// The running mission's task tree, and a race's course.
pub const MISSION_CHUNK: &str = concat!(
    "-- live:mission\n",
    guard!(),
    r#"local m=M.mrxplaystate.GetCurrentMission()
if m==nil then return "mission|-||||$" end
local rows={}
local race="-"
local locs=""
local function walk(o,d)
  if #rows>=40 or d>6 then return end
  local tm,st=-1,-1
  if o._oTimer then tm=o._oTimer:GetTime() or -1; st=o._oTimer.nStopTime or -1 end
  rows[#rows+1]=string.format("%d %s %d %d %d",d,tostring(o:GetName()),o._nState or 0,tm,st)
  if o._tCourseLocs then
    race=string.format("%d %d",o.iCurLoc or 0,#o._tCourseLocs)
    local t={}
    for i=1,math.min(#o._tCourseLocs,64) do
      local g=Pg.GetGuidByName(o._tCourseLocs[i])
      if g then
        local a,b,e=Object.GetPosition(g)
        t[#t+1]=string.format("%.2f %.2f %.2f",a,b,e)
      end
    end
    locs=table.concat(t,";")
  end
  for _,k in pairs(o._tChildren or {}) do walk(k,d+1) end
end
walk(m,0)
return string.format("mission|%s|%s|%s|%s|$",tostring(m:GetMissionId()),table.concat(rows,";"),race,locs)
"#
);

/// The level's static layers: fixed for a level, so once per session of the page.
pub const STATICS_CHUNK: &str = concat!(
    "-- live:statics\n",
    guard!(),
    r#"for _,v in pairs(M) do
  local t=rawget(v,"_tStaticLayers")
  if t then return "statics|"..table.concat(t,",").."|$" end
end
return "statics||$"
"#
);

/// The storage piles the game has hidden. `WifPmcInterior._SetStockpileCategoryQty` hides pile `i` of a
/// category when its quantity is under limit `i`, and this reads the same two tables. A category with
/// no quantity yet has not been through it, so it hides nothing.
pub const STOCK_CHUNK: &str = concat!(
    "-- live:stock\n",
    guard!(),
    r#"local p=M.wifpmcinterior
local l={}
for c,t in pairs(p._tStockpile) do
  local q=p._tStockpileQty[c]
  if q then
    for i,n in ipairs(t) do
      if q<n then l[#l+1]="pmcoutpost_stockpile_"..c.." "..i end
    end
  end
end
return "stock|"..table.concat(l,",").."|$"
"#
);

/// How many assets one ask carries: `M2Live.AssetStatus`'s limit.
const ASSETS_PER_ASK: usize = 256;

/// The residency of `slice`, `(name, type)` hashes. Hexadecimal text, because Lua's numbers are 32-bit
/// floats and would round a hash. These are asset hashes, not guids: an unknown one reads as absent.
pub fn assets_chunk(slice: &[(u32, u32)]) -> String {
    let list: Vec<String> = slice.iter().map(|(n, t)| format!("{n:08X}:{t:08X}")).collect();
    format!(
        "-- live:assets\n{}if M2Live==nil then return \"assets|-|$\" end\nreturn \"assets|\"..M2Live.AssetStatus(\"{}\")..\"|$\"\n",
        guard!(),
        list.join(",")
    )
}

/// Every engine or module call a chunk may make. All are reads.
#[cfg(test)]
const ALLOWED: &[&str] = &[
    "Ai.GetFactionGuid",
    "Camera.GetFOV",
    "Camera.GetPitch",
    "Camera.GetYaw",
    "Loader.GetLoadPhase",
    "M2Live.AssetStatus",
    "M2Live.CameraEye",
    "M2Live.Residency",
    "M.mrxplaystate.Get",
    "M.mrxplaystate.GetCurrentMission",
    "Object.GetHealth",
    "Object.GetLocalizedName",
    "Object.GetMaxHealth",
    "Object.GetModelName",
    "Object.GetName",
    "Object.GetParent",
    "Object.GetPosition",
    "Object.GetYaw",
    "Object.HasLabel",
    "Pg.FastCollectBoats",
    "Pg.FastCollectCars",
    "Pg.FastCollectHelicopters",
    "Pg.FastCollectHumans",
    "Pg.FastCollectJets",
    "Pg.FastCollectTanks",
    "Pg.GetGuidByName",
    "Player.GetCamera",
    "Player.GetLocalCharacter",
    "Player.GetLocalPlayer",
    "Sys.GetLevelName",
    "Sys.GuidToString",
    "Sys.IsLoadingOrStreaming",
    "Vehicle.GetFromRider",
    "f.GetLastCompletedContractName",
    "math.min",
    "string.format",
    "table.concat",
    "table.sort",
];

/// How the link stands.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum LinkState {
    /// Nothing answers at the address.
    #[default]
    Off,
    /// The game answers but has no world: the menu, or a load, at this rung of the ladder.
    NoWorld { phase: i32, name: String },
    /// A world, with a hero not yet created.
    NoHero,
    /// Following the game.
    Streaming,
    /// The port accepts and no answer came: a load, a hang, or a paused process.
    NotAnswering,
    /// A recording that cannot be played.
    Failed(String),
    /// Playing a recording back.
    Replay { path: PathBuf, finished: bool },
}

impl LinkState {
    pub fn up(&self) -> bool {
        !matches!(self, LinkState::Off)
    }
}

/// A vehicle's reported pose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VehiclePose {
    pub guid: String,
    pub pos: [f32; 3],
    pub yaw: f32,
}

/// The game camera's angles, in degrees. The game has no read of its position.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraAngles {
    pub yaw: f32,
    pub pitch: f32,
    pub fov: f32,
}

/// One pose sample, in the game's left-handed Y-up metres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    pub pos: [f32; 3],
    /// Degrees, as `Object.GetYaw` returns it.
    pub yaw: f32,
    pub health: f32,
    pub max_health: f32,
    pub vehicle: Option<VehiclePose>,
    pub camera: Option<CameraAngles>,
    /// The camera's eye: `M2Live.CameraEye`, absent on an m2-sdk without it.
    pub eye: Option<[f32; 3]>,
}

/// What an entity is, by the collector that found it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityClass {
    Hero,
    Human,
    Car,
    Tank,
    Helicopter,
    Jet,
    Boat,
}

impl EntityClass {
    fn parse(s: &str) -> Option<EntityClass> {
        Some(match s {
            "hero" => EntityClass::Hero,
            "human" => EntityClass::Human,
            "car" => EntityClass::Car,
            "tank" => EntityClass::Tank,
            "helicopter" => EntityClass::Helicopter,
            "jet" => EntityClass::Jet,
            "boat" => EntityClass::Boat,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            EntityClass::Hero => "hero",
            EntityClass::Human => "human",
            EntityClass::Car => "car",
            EntityClass::Tank => "tank",
            EntityClass::Helicopter => "helicopter",
            EntityClass::Jet => "jet",
            EntityClass::Boat => "boat",
        }
    }

    pub fn is_vehicle(self) -> bool {
        !matches!(self, EntityClass::Hero | EntityClass::Human)
    }
}

/// One reported entity. `guid` is text: it identifies the row and is never sent back to the game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub guid: String,
    pub class: EntityClass,
    pub pos: [f32; 3],
    pub yaw: f32,
    pub health: f32,
    pub max_health: f32,
    pub template: u32,
    pub model: u32,
    pub faction: u32,
    /// The string-table key `Object.GetLocalizedName` returns, `[0x…]`.
    pub name_key: String,
}

/// The level's state.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct WorldState {
    /// `retail` or `reimpl`.
    pub host: String,
    pub level: String,
    /// `Mattias`, `Jennifer`, `Chris`, or empty before the hero exists.
    pub hero: String,
    pub last_contract: String,
    /// 0 = free roam, 1 = a mission (`mrxplaystate.lua:7-8`).
    pub play_state: i32,
    pub blocking: i32,
    pub streaming: bool,
    pub jobs: u32,
    /// Dynamic layers marked for removal, not counted in `layers`.
    pub leaving: u32,
    /// The rung of the load ladder the bridge has seen, and its name; -1 before a load.
    pub phase: i32,
    pub phase_name: String,
    /// Records registered and blocks resident: `M2Live.Residency`, absent on an m2-sdk without it.
    pub residency: Option<(u32, u32)>,
    /// The dynamic layers loaded and staying.
    pub layers: Vec<String>,
}

/// One task of the running mission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub depth: u32,
    pub name: String,
    /// 0 latent, 1 active, 2 done, 3 cancelled (`mrxtaskstate.lua:1-4`).
    pub state: i32,
    pub time: Option<i32>,
    pub stop: Option<i32>,
}

/// A race's progress: `at` is the 1-based checkpoint being raced to, 0 before the start.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Race {
    pub at: u32,
    pub total: u32,
    pub points: Vec<[f32; 3]>,
}

/// The running mission.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Mission {
    /// Empty in free roam.
    pub id: String,
    pub tasks: Vec<Task>,
    pub race: Option<Race>,
}

/// What the registry says of one asked asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetState {
    /// No record.
    Absent,
    Record { status: u16, refs: u32 },
}

/// One parsed reply.
#[derive(Debug, Clone, PartialEq)]
pub enum Sample {
    NoWorld { phase: i32, name: String },
    NoHero,
    Pose(Pose),
    Entities(Vec<Entity>),
    World(WorldState),
    Mission(Mission),
    Statics(Vec<String>),
    Stock(Vec<String>),
    /// In the order asked; `None` on an m2-sdk without `M2Live`.
    Assets(Option<Vec<AssetState>>),
}

fn field<'a>(it: &mut impl Iterator<Item = &'a str>, what: &str) -> Result<&'a str, String> {
    it.next().ok_or_else(|| format!("no {what}"))
}

fn num<T: std::str::FromStr>(s: &str, what: &str) -> Result<T, String> {
    s.parse().map_err(|_| format!("{what} is {s:?}"))
}

fn floats<const N: usize>(s: &str, what: &str) -> Result<[f32; N], String> {
    let v: Vec<&str> = s.split(' ').collect();
    if v.len() != N {
        return Err(format!("{what} has {} values, not {N}: {s:?}", v.len()));
    }
    let mut out = [0.0; N];
    for (o, t) in out.iter_mut().zip(v) {
        *o = num(t, what)?;
    }
    Ok(out)
}

/// A hash field: `Sys.GuidToString` text, `0x` and eight digits.
fn hash32(s: &str, what: &str) -> Result<u32, String> {
    let hex = s.strip_prefix("0x").ok_or_else(|| format!("{what} is {s:?}"))?;
    u32::from_str_radix(hex, 16).map_err(|_| format!("{what} is {s:?}"))
}

fn parse_pose<'a>(it: &mut impl Iterator<Item = &'a str>) -> Result<Sample, String> {
    let [x, y, z, yaw, health, max_health] = floats::<6>(field(it, "pose")?, "pose")?;
    let vehicle = match field(it, "vehicle")? {
        "-" => None,
        v => {
            let (guid, rest) = v.split_once(' ').ok_or_else(|| format!("vehicle is {v:?}"))?;
            let [a, b, c, yaw] = floats::<4>(rest, "vehicle")?;
            Some(VehiclePose { guid: guid.to_string(), pos: [a, b, c], yaw })
        }
    };
    let camera = match field(it, "camera")? {
        "-" => None,
        c => {
            let [yaw, pitch, fov] = floats::<3>(c, "camera")?;
            Some(CameraAngles { yaw, pitch, fov })
        }
    };
    let eye = match field(it, "camera eye")? {
        "-" => None,
        e => Some(floats::<3>(e, "camera eye")?),
    };
    Ok(Sample::Pose(Pose { pos: [x, y, z], yaw, health, max_health, vehicle, camera, eye }))
}

fn parse_entity(row: &str) -> Result<Entity, String> {
    let f: Vec<&str> = row.split(' ').collect();
    let [guid, class, x, y, z, yaw, health, max_health, template, model, faction, name_key] = f[..] else {
        return Err(format!("entity row has {} fields, not 12: {row:?}", f.len()));
    };
    Ok(Entity {
        guid: guid.to_string(),
        class: EntityClass::parse(class).ok_or_else(|| format!("entity class is {class:?}"))?,
        pos: [num(x, "x")?, num(y, "y")?, num(z, "z")?],
        yaw: num(yaw, "yaw")?,
        health: num(health, "health")?,
        max_health: num(max_health, "max health")?,
        template: hash32(template, "template")?,
        model: hash32(model, "model")?,
        faction: hash32(faction, "faction")?,
        name_key: name_key.to_string(),
    })
}

fn list(s: &str) -> Vec<String> {
    s.split(',').filter(|t| !t.is_empty()).map(str::to_string).collect()
}

fn parse_world<'a>(it: &mut impl Iterator<Item = &'a str>) -> Result<Sample, String> {
    let host = field(it, "host")?.to_string();
    let level = field(it, "level")?.to_string();
    let hero = match field(it, "hero")? {
        "-" => String::new(),
        h => h.to_string(),
    };
    let last_contract = match field(it, "last contract")? {
        "nil" => String::new(),
        c => c.to_string(),
    };
    Ok(Sample::World(WorldState {
        host,
        level,
        hero,
        last_contract,
        play_state: num(field(it, "play state")?, "play state")?,
        blocking: num(field(it, "blocking")?, "blocking")?,
        streaming: field(it, "streaming")? == "1",
        jobs: num(field(it, "jobs")?, "jobs")?,
        leaving: num(field(it, "leaving")?, "leaving")?,
        phase: num(field(it, "phase")?, "phase")?,
        phase_name: field(it, "phase name")?.to_string(),
        residency: match field(it, "residency")? {
            "-" => None,
            r => {
                let (a, b) = r.split_once(' ').ok_or_else(|| format!("residency is {r:?}"))?;
                Some((num(a, "records")?, num(b, "blocks")?))
            }
        },
        layers: list(field(it, "layers")?),
    }))
}

fn parse_task(row: &str) -> Result<Task, String> {
    let f: Vec<&str> = row.split(' ').collect();
    let [depth, name, state, time, stop] = f[..] else {
        return Err(format!("task row has {} fields, not 5: {row:?}", f.len()));
    };
    let timer = |s: &str, what: &str| -> Result<Option<i32>, String> {
        let v: i32 = num(s, what)?;
        Ok((v >= 0).then_some(v))
    };
    Ok(Task {
        depth: num(depth, "depth")?,
        name: name.to_string(),
        state: num(state, "state")?,
        time: timer(time, "time")?,
        stop: timer(stop, "stop")?,
    })
}

fn parse_mission<'a>(it: &mut impl Iterator<Item = &'a str>) -> Result<Sample, String> {
    let id = field(it, "mission id")?;
    let rows = field(it, "tasks")?;
    let race = field(it, "race")?;
    let locs = field(it, "course")?;
    if id == "-" {
        return Ok(Sample::Mission(Mission::default()));
    }
    let tasks = rows.split(';').filter(|r| !r.is_empty()).map(parse_task).collect::<Result<_, _>>()?;
    let race = match race {
        "-" | "" => None,
        r => {
            let (at, total) = r.split_once(' ').ok_or_else(|| format!("race is {r:?}"))?;
            let points = locs.split(';').filter(|l| !l.is_empty()).map(|l| floats::<3>(l, "checkpoint")).collect::<Result<_, _>>()?;
            Some(Race { at: num(at, "race position")?, total: num(total, "race length")?, points })
        }
    };
    Ok(Sample::Mission(Mission { id: id.to_string(), tasks, race }))
}

fn parse_asset(s: &str) -> Result<AssetState, String> {
    if s == "-" {
        return Ok(AssetState::Absent);
    }
    let (status, refs) = s.split_once(':').ok_or_else(|| format!("asset is {s:?}"))?;
    Ok(AssetState::Record { status: num(status, "asset status")?, refs: num(refs, "asset references")? })
}

/// Parse a reply's text, as [`Bridge::eval`] returned it. Anything that is not whole, one of the
/// expected shapes, is an error that names what was wrong.
pub fn parse_sample(text: &str) -> Result<Sample, String> {
    let body = match parse_reply(text) {
        Reply::Ok(v) => match v.as_slice() {
            [Value::Str(s)] => s.clone(),
            // An open quote with no close: the bridge's limit took the rest of the string.
            [Value::Other(s)] if s.starts_with('"') => {
                return Err(format!("the reply was cut at the bridge's limit ({} bytes)", s.len()))
            }
            other => return Err(format!("the chunk returned {other:?}, not one string")),
        },
        Reply::Runtime(e) => return Err(format!("the chunk failed: {e}")),
        Reply::Compile(e) => return Err(format!("the chunk did not compile: {e}")),
        Reply::Bridge(e) => return Err(format!("the bridge refused the chunk: {e}")),
        Reply::Unrecognized(t) => {
            return Err(format!("the reply is cut short or not from a bridge: {:?}", t.chars().take(60).collect::<String>()))
        }
    };
    let Some(body) = body.strip_suffix(END).and_then(|b| b.strip_suffix('|')) else {
        return Err(format!("the reply has no end marker ({} bytes)", body.len()));
    };
    let mut it = body.split('|');
    match field(&mut it, "kind")? {
        "noworld" => {
            let phase = num(field(&mut it, "load phase")?, "load phase")?;
            Ok(Sample::NoWorld { phase, name: field(&mut it, "phase name")?.to_string() })
        }
        "nohero" => Ok(Sample::NoHero),
        "pose" => parse_pose(&mut it),
        "entities" => {
            Ok(Sample::Entities(field(&mut it, "entities")?.split(';').map(parse_entity).collect::<Result<_, _>>()?))
        }
        "world" => parse_world(&mut it),
        "mission" => parse_mission(&mut it),
        "statics" => Ok(Sample::Statics(list(field(&mut it, "static layers")?))),
        "stock" => Ok(Sample::Stock(list(field(&mut it, "hidden piles")?))),
        "assets" => match field(&mut it, "assets")? {
            "-" if body == "assets|-" => Ok(Sample::Assets(None)),
            rows => rows.split(',').map(parse_asset).collect::<Result<_, _>>().map(|v| Sample::Assets(Some(v))),
        },
        k => Err(format!("unknown reply kind {k:?}")),
    }
}

/// Everything the page shows from the game, as last heard.
#[derive(Debug, Clone, Default)]
pub struct Live {
    pub state: LinkState,
    /// Round trip of the last pose ask.
    pub rtt: Option<Duration>,
    /// The two newest pose samples, oldest first, with when each arrived.
    pub poses: Vec<(Instant, Pose)>,
    pub entities: Vec<Entity>,
    pub world: Option<WorldState>,
    pub mission: Mission,
    pub statics: Vec<String>,
    /// The storage piles the game has hidden, by name.
    pub hidden_piles: Vec<String>,
    /// Which address was asked.
    pub addr: String,
    /// The recording in progress.
    pub recording: Option<PathBuf>,
    /// A reply that could not be used, by what it was for. It stands until that tier answers again, so
    /// a chunk that keeps failing stays on screen and the others carry on.
    pub faults: Vec<(String, String)>,
    /// The overlay's residency; `None` until read, or on an m2-sdk without `M2Live`.
    pub assets: Option<AssetTally>,
}

/// The asked assets, over all the slices read so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AssetTally {
    pub total: usize,
    /// Status 2.
    pub resident: usize,
    /// A record, not resident.
    pub registered: usize,
    /// Resident on an earlier read, gone on the last.
    pub evicted: usize,
}

/// What each asset last read as, and which have ever been resident: one read cannot show an eviction.
#[derive(Debug, Default)]
struct AssetBook {
    list: Vec<(u32, u32)>,
    last: HashMap<(u32, u32), AssetState>,
    seen: HashSet<(u32, u32)>,
    /// The start of the next slice.
    at: usize,
}

impl AssetBook {
    fn next_slice(&self) -> &[(u32, u32)] {
        &self.list[self.at..(self.at + ASSETS_PER_ASK).min(self.list.len())]
    }

    /// `states` answers [`next_slice`](Self::next_slice).
    fn take(&mut self, states: &[AssetState]) {
        let slice = self.next_slice().to_vec();
        for (key, state) in slice.iter().zip(states) {
            self.last.insert(*key, *state);
            if matches!(state, AssetState::Record { status: STATUS_RESIDENT, .. }) {
                self.seen.insert(*key);
            }
        }
        self.at += slice.len();
        if self.at >= self.list.len() {
            self.at = 0;
        }
    }

    fn tally(&self) -> AssetTally {
        let mut t = AssetTally { total: self.list.len(), ..AssetTally::default() };
        for (key, state) in &self.last {
            match state {
                AssetState::Record { status: STATUS_RESIDENT, .. } => t.resident += 1,
                AssetState::Record { .. } => t.registered += 1,
                AssetState::Absent => t.evicted += usize::from(self.seen.contains(key)),
            }
        }
        t
    }
}

impl Live {
    /// What is known of the game is gone; which game was asked, and any recording, are not.
    fn forget_game(&mut self, state: LinkState) {
        *self = Live { state, addr: std::mem::take(&mut self.addr), recording: self.recording.take(), ..Live::default() };
    }

    fn fault(&mut self, of: &str, why: String) {
        match self.faults.iter_mut().find(|(k, _)| k == of) {
            Some(f) => f.1 = why,
            None => self.faults.push((of.to_string(), why)),
        }
    }

    fn clear_fault(&mut self, of: &str) {
        self.faults.retain(|(k, _)| k != of);
    }

    fn apply(&mut self, s: Sample, at: Instant) {
        match s {
            Sample::NoWorld { phase, name } => {
                let state = match &self.state {
                    LinkState::Replay { .. } => self.state.clone(),
                    _ => LinkState::NoWorld { phase, name },
                };
                self.forget_game(state);
            }
            Sample::NoHero => self.note(LinkState::NoHero),
            Sample::Pose(p) => {
                self.note(LinkState::Streaming);
                if self.poses.len() == 2 {
                    self.poses.remove(0);
                }
                self.poses.push((at, p));
            }
            Sample::Entities(e) => self.entities = e,
            Sample::World(w) => self.world = Some(w),
            Sample::Mission(m) => self.mission = m,
            Sample::Statics(l) => self.statics = l,
            Sample::Stock(l) => self.hidden_piles = l,
            // The worker keeps the book, which needs the slice the reply answers.
            Sample::Assets(_) => {}
        }
    }

    /// A replay's state stands through its samples; a live reply sets the link's.
    fn note(&mut self, state: LinkState) {
        if !matches!(self.state, LinkState::Replay { .. }) {
            self.state = state;
        }
    }
}

/// One recorded reply.
#[derive(Debug, Serialize, Deserialize)]
struct Recorded {
    /// Milliseconds since the recording began.
    t: u64,
    tier: Tier,
    reply: String,
}

/// A recording in progress.
struct Recorder {
    out: std::io::BufWriter<std::fs::File>,
    start: Instant,
}

impl Recorder {
    fn write(&mut self, tier: Tier, reply: &str) -> Result<(), String> {
        let line = Recorded { t: self.start.elapsed().as_millis() as u64, tier, reply: reply.to_string() };
        let json = serde_json::to_string(&line).map_err(|e| e.to_string())?;
        writeln!(self.out, "{json}").map_err(|e| e.to_string())?;
        self.out.flush().map_err(|e| e.to_string())
    }
}

/// What the page asks of the worker.
#[derive(Default)]
struct Requests {
    record_to: Option<PathBuf>,
    stop_recording: bool,
    replay: Option<PathBuf>,
    leave_replay: bool,
    assets: Option<Vec<(u32, u32)>>,
}

/// The handle the app holds. The worker lives as long as the app.
pub struct Link {
    live: Arc<Mutex<Live>>,
    /// The page is open: hold a session and poll. Closed: only check the port.
    open: Arc<AtomicBool>,
    reqs: Arc<Mutex<Requests>>,
}

impl Link {
    /// Start the worker, asking `addr`.
    pub fn spawn(addr: String) -> Link {
        let live = Arc::new(Mutex::new(Live { addr, ..Live::default() }));
        let open = Arc::new(AtomicBool::new(false));
        let reqs = Arc::new(Mutex::new(Requests::default()));
        let worker = Worker { live: live.clone(), open: open.clone(), reqs: reqs.clone(), recorder: None, book: AssetBook::default() };
        std::thread::Builder::new()
            .name("live-link".into())
            .spawn(move || worker.run())
            .expect("spawn the live link thread");
        Link { live, open, reqs }
    }

    /// The page opened or closed.
    pub fn set_open(&self, open: bool) {
        self.open.store(open, Ordering::Relaxed);
    }

    /// The `(name, type)` assets to follow the residency of.
    pub fn set_assets(&self, assets: Vec<(u32, u32)>) {
        self.reqs.lock().expect("the live requests").assets = Some(assets);
    }

    /// Ask `addr` from the next connection on.
    pub fn set_addr(&self, addr: String) {
        self.live.lock().expect("the live state").addr = addr;
    }

    /// The rail dot: the game answers at the address.
    pub fn up(&self) -> bool {
        let live = self.live.lock().expect("the live state");
        live.state.up() && !matches!(live.state, LinkState::Replay { .. })
    }

    /// A copy of what is known now.
    pub fn snapshot(&self) -> Live {
        self.live.lock().expect("the live state").clone()
    }

    /// Start recording replies to `path`.
    pub fn record_to(&self, path: PathBuf) {
        self.reqs.lock().expect("the live requests").record_to = Some(path);
    }

    /// Stop the recording.
    pub fn stop_recording(&self) {
        self.reqs.lock().expect("the live requests").stop_recording = true;
    }

    /// Play a recording in place of the game.
    pub fn replay(&self, path: PathBuf) {
        self.reqs.lock().expect("the live requests").replay = Some(path);
    }

    /// Stop a replay and go back to the game.
    pub fn leave_replay(&self) {
        self.reqs.lock().expect("the live requests").leave_replay = true;
    }
}

/// The recorded replies of `path`, in order.
fn read_replay(path: &Path) -> Result<Vec<Recorded>, String> {
    let f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    std::io::BufReader::new(f)
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let line = line.map_err(|e| format!("{}: {e}", path.display()))?;
            serde_json::from_str(&line).map_err(|e| format!("{}:{}: {e}", path.display(), i + 1))
        })
        .collect()
}

struct Worker {
    live: Arc<Mutex<Live>>,
    open: Arc<AtomicBool>,
    reqs: Arc<Mutex<Requests>>,
    recorder: Option<Recorder>,
    book: AssetBook,
}

impl Worker {
    fn run(mut self) {
        loop {
            self.requests();
            if let Some(path) = self.take_replay() {
                self.play(&path);
            } else if self.open.load(Ordering::Relaxed) {
                self.session();
            } else {
                self.idle();
            }
        }
    }

    fn set_state(&self, state: LinkState) {
        self.live.lock().expect("the live state").state = state;
    }

    fn addr(&self) -> String {
        self.live.lock().expect("the live state").addr.clone()
    }

    fn take_replay(&self) -> Option<PathBuf> {
        self.reqs.lock().expect("the live requests").replay.take()
    }

    /// Act on the recording requests; a recording that cannot be written ends and says why.
    fn requests(&mut self) {
        let (to, stop, assets) = {
            let mut r = self.reqs.lock().expect("the live requests");
            (r.record_to.take(), std::mem::take(&mut r.stop_recording), r.assets.take())
        };
        if let Some(list) = assets {
            self.book = AssetBook { list, ..AssetBook::default() };
        }
        if stop {
            self.recorder = None;
            self.live.lock().expect("the live state").recording = None;
        }
        if let Some(path) = to {
            match std::fs::File::create(&path) {
                Ok(f) => {
                    self.recorder = Some(Recorder { out: std::io::BufWriter::new(f), start: Instant::now() });
                    self.live.lock().expect("the live state").recording = Some(path);
                }
                Err(e) => self.live.lock().expect("the live state").fault("recording", format!("{}: {e}", path.display())),
            }
        }
    }

    /// The page is closed: keep the rail dot true, and run nothing in the game. Waits in short steps, so
    /// the page opening or a replay starts at once. A failed recording stays until the next one.
    fn idle(&self) {
        let up = mercs2_bridge::reachable(self.addr().as_str(), IDLE_CONNECT);
        {
            let mut live = self.live.lock().expect("the live state");
            if !matches!(live.state, LinkState::Failed(_)) && up != live.state.up() {
                let state = if up { LinkState::NotAnswering } else { LinkState::Off };
                live.forget_game(state);
            }
        }
        let until = Instant::now() + IDLE_POLL;
        while Instant::now() < until && !self.open.load(Ordering::Relaxed) {
            if self.reqs.lock().expect("the live requests").replay.is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Hold one session for [`SESSION`], then close it, so a REPL client waiting on the slot gets in.
    fn session(&mut self) {
        let mut bridge = match Bridge::connect(self.addr().as_str(), MIN_SEND, ANSWER) {
            Ok(b) => b,
            Err(_) => {
                self.live.lock().expect("the live state").forget_game(LinkState::Off);
                std::thread::sleep(RETRY);
                return;
            }
        };
        let began = Instant::now();
        let mut schedule = Schedule::new(began);
        // A level's static list does not change, so a rotation does not ask for it again.
        schedule.statics_done = !self.live.lock().expect("the live state").statics.is_empty();
        while began.elapsed() < SESSION && self.open.load(Ordering::Relaxed) {
            self.requests();
            if self.reqs.lock().expect("the live requests").replay.is_some() {
                return;
            }
            let now = Instant::now();
            let Some(tier) = schedule.due(now) else {
                std::thread::sleep(schedule.until_next(now));
                continue;
            };
            schedule.asked(tier, now);
            if tier == Tier::Assets && self.book.list.is_empty() {
                continue;
            }
            let sent = Instant::now();
            let asked = if tier == Tier::Assets { bridge.eval(&assets_chunk(self.book.next_slice())) } else { bridge.eval(tier.chunk()) };
            match asked {
                Ok(text) => self.take_reply(tier, &text, sent),
                Err(BridgeError::Timeout) => {
                    self.set_state(LinkState::NotAnswering);
                    return;
                }
                Err(_) => {
                    self.live.lock().expect("the live state").forget_game(LinkState::Off);
                    std::thread::sleep(RETRY);
                    return;
                }
            }
        }
    }

    /// Record and apply one reply. One that cannot be used stands as that tier's fault.
    fn take_reply(&mut self, tier: Tier, text: &str, sent: Instant) {
        if let Some(r) = self.recorder.as_mut() {
            if let Err(e) = r.write(tier, text) {
                self.recorder = None;
                let mut live = self.live.lock().expect("the live state");
                live.recording = None;
                live.fault("recording", e);
            }
        }
        let name = format!("{tier:?}");
        let mut live = self.live.lock().expect("the live state");
        match parse_sample(text) {
            Ok(s) => {
                if tier == Tier::Pose {
                    live.rtt = Some(sent.elapsed());
                }
                if let Sample::Assets(Some(states)) = &s {
                    self.book.take(states);
                    live.assets = Some(self.book.tally());
                }
                live.apply(s, Instant::now());
                live.clear_fault(&name);
            }
            Err(e) => live.fault(&name, e),
        }
    }

    /// Feed a recording through the path the game's replies take, at the pace it was recorded.
    fn play(&mut self, path: &Path) {
        let replies = match read_replay(path) {
            Ok(r) => r,
            Err(e) => {
                self.set_state(LinkState::Failed(e));
                return;
            }
        };
        self.live.lock().expect("the live state").forget_game(LinkState::Replay { path: path.to_path_buf(), finished: false });
        let start = Instant::now();
        for r in &replies {
            let at = start + Duration::from_millis(r.t);
            while Instant::now() < at {
                if self.leave_replay() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5).min(at.saturating_duration_since(Instant::now())));
            }
            match parse_sample(&r.reply) {
                Ok(s) => self.live.lock().expect("the live state").apply(s, Instant::now()),
                Err(e) => {
                    self.set_state(LinkState::Failed(format!("{}: {e}", path.display())));
                    return;
                }
            }
        }
        self.set_state(LinkState::Replay { path: path.to_path_buf(), finished: true });
        while !self.leave_replay() {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn leave_replay(&self) -> bool {
        let mut r = self.reqs.lock().expect("the live requests");
        let leaving = std::mem::take(&mut r.leave_replay) || r.replay.is_some();
        if leaving {
            self.live.lock().expect("the live state").state = LinkState::Off;
        }
        leaving
    }
}

/// Where the bridge in `install` listens: `[repl] host` and `port` of `scripts/lua_bridge.ini`, as the
/// ASI reads them, its own defaults when the file or a key is absent.
pub fn bridge_addr(install: Option<&Path>) -> String {
    let text = install.and_then(|d| std::fs::read(d.join("scripts").join("lua_bridge.ini")).ok());
    let (mut host, mut port) = mercs2_bridge::DEFAULT_ADDR.rsplit_once(':').map(|(h, p)| (h.to_string(), p.to_string())).expect("the default address has a port");
    if let Some(text) = text {
        mercs2_formats::ini::parse(&text, |k, v| match k {
            b"host" => host = String::from_utf8_lossy(v).into_owned(),
            b"port" => port = String::from_utf8_lossy(v).into_owned(),
            _ => {}
        });
    }
    format!("{host}:{port}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reply as the bridge frames one string.
    fn ok(s: &str) -> String {
        format!("[ok]\t\"{s}\"")
    }

    fn chunks() -> Vec<String> {
        let mut c: Vec<String> = [POSE_CHUNK, ENTITIES_CHUNK, WORLD_CHUNK, STOCK_CHUNK, MISSION_CHUNK, STATICS_CHUNK].map(str::to_string).to_vec();
        c.push(assets_chunk(&[(0x25C9_8327, 0x5B72_4250)]));
        c
    }

    /// Every `a.b(` or `a.b.c(` call in a chunk, method calls excluded.
    fn dotted_calls(chunk: &str) -> Vec<String> {
        let bytes = chunk.as_bytes();
        let mut out = Vec::new();
        for (i, &b) in bytes.iter().enumerate() {
            if b != b'(' {
                continue;
            }
            let mut s = i;
            while s > 0 && (bytes[s - 1].is_ascii_alphanumeric() || matches!(bytes[s - 1], b'_' | b'.')) {
                s -= 1;
            }
            // A run of dots ahead of the name is Lua's concat operator, `"…"..table.concat(`.
            let name = chunk[s..i].trim_start_matches('.');
            if name.contains('.') {
                out.push(name.to_string());
            }
        }
        out
    }

    /// ★ The page's promise: a chunk can only read. A call off the list, or one of the names known to
    /// write, allocate or log, fails here.
    #[test]
    fn read_only_chunks() {
        for chunk in chunks() {
            for call in dotted_calls(&chunk) {
                assert!(ALLOWED.contains(&call.as_str()), "{call} is not on the read-only list");
            }
            for banned in ["Debug.Printf", "Sys.MainTimeStamp", "PopKeyEvents", "import(", "print(", "SaveVar", "Loader.LoadFile", "Tcp.Send", ".Set", ":Set", "Spawn", "Remove"] {
                assert!(!chunk.contains(banned), "a chunk uses {banned}");
            }
        }
    }

    /// A method call on a game object must be a known getter too.
    #[test]
    fn chunks_call_only_getters_on_objects() {
        for chunk in chunks() {
            for (i, _) in chunk.match_indices(':') {
                let rest = &chunk[i + 1..];
                let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if rest[name.len()..].starts_with('(') {
                    assert!(["GetTime", "GetName", "GetMissionId"].contains(&name.as_str()), ":{name} is not a known getter");
                }
            }
        }
    }

    /// A stale guid in a native read can kill the game, so no chunk takes a value from the page.
    #[test]
    fn chunks_are_constants_and_guard_the_world() {
        for chunk in chunks() {
            assert!(chunk.contains("M.mrxplaystate==nil"), "a chunk has no world guard");
            assert!(chunk.lines().next().expect("a first line").starts_with("-- live:"), "a chunk has no name for lua_bridge.log");
        }
    }

    #[test]
    fn a_stock_reply_names_the_hidden_piles() {
        let s = parse_sample(&ok("stock|pmcoutpost_stockpile_money 8,pmcoutpost_stockpile_moab 1|$")).unwrap();
        let Sample::Stock(l) = s else { panic!("not stock: {s:?}") };
        assert_eq!(l, ["pmcoutpost_stockpile_money 8", "pmcoutpost_stockpile_moab 1"]);
        let Sample::Stock(none) = parse_sample(&ok("stock||$")).unwrap() else { panic!("not stock") };
        assert!(none.is_empty());
    }

    #[test]
    fn a_pose_reads_as_the_game_sends_it() {
        // Measured from the running game, 2026-11-01.
        let s = parse_sample(&ok("pose|2635.098 -13.822 -998.986 72.992 120.0 120.0|-|-6.236 -39.978 54.990|-|$")).unwrap();
        let Sample::Pose(p) = s else { panic!("not a pose: {s:?}") };
        assert_eq!(p.pos, [2635.098, -13.822, -998.986]);
        assert_eq!(p.yaw, 72.992);
        assert_eq!(p.vehicle, None);
        assert_eq!(p.camera, Some(CameraAngles { yaw: -6.236, pitch: -39.978, fov: 54.990 }));
        assert_eq!(p.eye, None, "an m2-sdk without M2Live has no eye");
    }

    #[test]
    fn a_pose_carries_the_eye_when_the_sdk_reads_it() {
        let Sample::Pose(p) = parse_sample(&ok("pose|1 2 3 4 5 6|-|7 8 9|2633.0 -11.7 -996.8|$")).unwrap() else { panic!("not a pose") };
        assert_eq!(p.eye, Some([2633.0, -11.7, -996.8]));
    }

    #[test]
    fn a_pose_carries_the_vehicle() {
        let s = parse_sample(&ok("pose|1 2 3 4 5 6|0x40005C68 7 8 9 10|-|-|$")).unwrap();
        let Sample::Pose(p) = s else { panic!("not a pose") };
        assert_eq!(p.vehicle, Some(VehiclePose { guid: "0x40005C68".into(), pos: [7.0, 8.0, 9.0], yaw: 10.0 }));
        assert_eq!(p.camera, None);
    }

    #[test]
    fn an_entity_row_reads_as_the_game_sends_it() {
        let s = parse_sample(&ok("entities|0x40005C68 car 2653.79 -13.78 -996.73 -137.5 100.0 100.0 0x753E9DA1 0x9F5A4DB7 0x800056C4 [0xb7f587a3]|$")).unwrap();
        let Sample::Entities(e) = s else { panic!("not entities") };
        assert_eq!(e[0].class, EntityClass::Car);
        assert_eq!(e[0].template, 0x753E9DA1);
        assert_eq!(e[0].model, 0x9F5A4DB7);
        assert_eq!(e[0].name_key, "[0xb7f587a3]");
    }

    #[test]
    fn world_state_splits_layers_and_reads_the_counters() {
        let s = parse_sample(&ok("world|retail|vz|Mattias|PmcJob001|0|2|0|5|1|20|World fully loaded (GlobalExit)|10499 653|vz_a,vz_b|$")).unwrap();
        let Sample::World(w) = s else { panic!("not world") };
        assert_eq!(w.residency, Some((10499, 653)));
        assert_eq!((w.host.as_str(), w.level.as_str(), w.hero.as_str()), ("retail", "vz", "Mattias"));
        assert_eq!((w.blocking, w.jobs, w.leaving), (2, 5, 1));
        assert_eq!((w.phase, w.phase_name.as_str()), (20, "World fully loaded (GlobalExit)"));
        assert_eq!(w.layers, ["vz_a", "vz_b"]);
    }

    #[test]
    fn free_roam_is_an_empty_mission() {
        assert_eq!(parse_sample(&ok("mission|-||||$")).unwrap(), Sample::Mission(Mission::default()));
    }

    #[test]
    fn a_race_reads_its_course_and_timer() {
        let s = parse_sample(&ok("mission|PmcCon016|0 PmcCon016 1 -1 -1;1 Race 1 48 75|7 27|1 2 3;4 5 6|$")).unwrap();
        let Sample::Mission(m) = s else { panic!("not a mission") };
        assert_eq!(m.id, "PmcCon016");
        assert_eq!(m.tasks[0].time, None);
        assert_eq!((m.tasks[1].time, m.tasks[1].stop), (Some(48), Some(75)));
        let r = m.race.unwrap();
        assert_eq!((r.at, r.total), (7, 27));
        assert_eq!(r.points, [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
    }

    #[test]
    fn assets_read_in_the_order_asked() {
        assert_eq!(parse_sample(&ok("assets|2:5,-|$")).unwrap(), Sample::Assets(Some(vec![AssetState::Record { status: 2, refs: 5 }, AssetState::Absent])));
        assert_eq!(parse_sample(&ok("assets|-|$")).unwrap(), Sample::Assets(None), "an m2-sdk without M2Live");
    }

    #[test]
    fn hashes_travel_as_text() {
        assert!(assets_chunk(&[(0x25C9_8327, 0x5B72_4250), (1, 2)]).contains("25C98327:5B724250,00000001:00000002"));
    }

    /// One read cannot show an eviction: it takes a resident asset going absent.
    #[test]
    fn the_book_counts_an_eviction_across_reads() {
        let mut b = AssetBook { list: vec![(1, 1), (2, 1), (3, 1)], ..AssetBook::default() };
        b.take(&[AssetState::Record { status: 2, refs: 1 }, AssetState::Record { status: 1, refs: 0 }, AssetState::Absent]);
        assert_eq!(b.tally(), AssetTally { total: 3, resident: 1, registered: 1, evicted: 0 });
        b.take(&[AssetState::Absent, AssetState::Record { status: 1, refs: 0 }, AssetState::Absent]);
        assert_eq!(b.tally(), AssetTally { total: 3, resident: 0, registered: 1, evicted: 1 });
    }

    #[test]
    fn the_book_walks_a_long_list_in_slices() {
        let mut b = AssetBook { list: (0..600).map(|i| (i, 1)).collect(), ..AssetBook::default() };
        for expected in [ASSETS_PER_ASK, ASSETS_PER_ASK, 600 - 2 * ASSETS_PER_ASK, ASSETS_PER_ASK] {
            let n = b.next_slice().len();
            assert_eq!(n, expected);
            b.take(&vec![AssetState::Absent; n]);
        }
    }

    #[test]
    fn the_menu_is_no_world_at_its_rung() {
        assert_eq!(
            parse_sample(&ok("noworld|7|Soundbanks ready|$")).unwrap(),
            Sample::NoWorld { phase: 7, name: "Soundbanks ready".into() }
        );
    }

    /// The bridge cuts at 16 KiB with no marker: the close quote and the end marker are both gone.
    #[test]
    fn a_cut_reply_is_an_error_not_short_data() {
        let cut = "[ok]\t\"entities|0x40005C68 car 2653.79 -13.78";
        assert!(parse_sample(cut).unwrap_err().contains("cut"), "{:?}", parse_sample(cut));
        assert!(parse_sample(&ok("entities|x")).unwrap_err().contains("end marker"));
    }

    #[test]
    fn each_failure_tag_is_an_error_that_says_so() {
        assert!(parse_sample("[runtime] \"boom\"").unwrap_err().contains("boom"));
        assert!(parse_sample("[compile] \"bad\"").unwrap_err().contains("compile"));
        assert!(parse_sample("[bridge] chunk rejected").unwrap_err().contains("refused"));
        assert!(parse_sample("[ok]").unwrap_err().contains("one string"));
    }

    #[test]
    fn a_bad_field_names_itself() {
        let e = parse_sample(&ok("pose|1 2 3|-|-|$")).unwrap_err();
        assert!(e.contains("pose has 3 values"), "{e}");
        let e = parse_sample(&ok("entities|0x1 ufo 1 2 3 4 5 6 0x1 0x2 0x3 k|$")).unwrap_err();
        assert!(e.contains("ufo"), "{e}");
    }

    #[test]
    fn the_schedule_runs_each_tier_at_its_period_and_never_piles_up() {
        let t0 = Instant::now();
        let mut s = Schedule::new(t0);
        // Everything is due at once: the statics first, then in table order.
        assert_eq!(s.due(t0), Some(Tier::Statics));
        s.asked(Tier::Statics, t0);
        assert_eq!(s.due(t0), Some(Tier::Pose));
        s.asked(Tier::Pose, t0);
        assert_eq!(s.due(t0), Some(Tier::Entities));
        s.asked(Tier::Entities, t0);
        s.asked(Tier::World, t0);
        s.asked(Tier::Stock, t0);
        s.asked(Tier::Mission, t0);
        s.asked(Tier::Assets, t0);
        assert_eq!(s.due(t0), None, "the statics are asked once");
        assert_eq!(s.due(t0 + Duration::from_millis(99)), None);
        assert_eq!(s.due(t0 + Duration::from_millis(100)), Some(Tier::Pose));
        // Ten pose periods late is one ask, not ten.
        let late = t0 + Duration::from_secs(1);
        s.asked(Tier::Pose, late);
        assert_eq!(s.next.iter().find(|(t, _)| *t == Tier::Pose).unwrap().1, late + Duration::from_millis(100));
    }

    #[test]
    fn the_address_comes_from_the_install() {
        let dir = std::env::temp_dir().join(format!("live_link_addr_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("scripts/lua_bridge.ini"), "[repl]\nhost = 127.0.0.2\nport = 27999\n").unwrap();
        assert_eq!(bridge_addr(Some(&dir)), "127.0.0.2:27999");
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(bridge_addr(None), mercs2_bridge::DEFAULT_ADDR);
    }

    fn wait_for(link: &Link, what: &str, ok: impl Fn(&Live) -> bool) -> Live {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let s = link.snapshot();
            if ok(&s) {
                return s;
            }
            assert!(Instant::now() < deadline, "never reached {what}: {:?}", s.state);
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// A host that answers each chunk by the name on its first line.
    fn fake_game(pose: &'static str) -> String {
        let server = mercs2_bridge::Server::bind("127.0.0.1:0").expect("bind ephemeral");
        let addr = server.local_addr().expect("addr").to_string();
        std::thread::spawn(move || {
            let _ = server.serve(move |chunk| {
                let name = chunk.lines().next().unwrap_or_default();
                ok(match name {
                    "-- live:pose" => pose,
                    "-- live:entities" => "entities|0x1 hero 1 2 3 4 5 6 0xA 0xB 0xC [0x1]|$",
                    "-- live:world" => "world|retail|vz|Mattias|-|0|0|0|5|0|20|World fully loaded|-|vz_a|$",
                    "-- live:mission" => "mission|-||||$",
                    _ => "statics|vz_base|$",
                })
            });
        });
        addr
    }

    #[test]
    fn the_link_follows_a_host_through_every_tier() {
        let link = Link::spawn(fake_game("pose|1 2 3 4 5 6|-|-|-|$"));
        link.set_open(true);
        let s = wait_for(&link, "all tiers", |l| l.state == LinkState::Streaming && l.world.is_some() && !l.entities.is_empty() && !l.statics.is_empty());
        assert_eq!(s.poses.last().unwrap().1.pos, [1.0, 2.0, 3.0]);
        assert!(s.rtt.is_some());
        assert!(link.up());
    }

    #[test]
    fn a_host_with_no_world_says_so() {
        let link = Link::spawn(fake_game("noworld|7|Soundbanks ready|$"));
        link.set_open(true);
        wait_for(&link, "no world", |l| matches!(&l.state, LinkState::NoWorld { phase: 7, .. }));
    }

    #[test]
    fn a_silent_game_is_not_answering() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        std::thread::spawn(move || {
            let held: Vec<_> = listener.incoming().collect();
            drop(held);
        });
        let link = Link::spawn(addr);
        link.set_open(true);
        wait_for(&link, "not answering", |l| l.state == LinkState::NotAnswering);
    }

    /// A chunk that keeps failing stays on screen, and the other tiers carry on.
    #[test]
    fn a_reply_it_cannot_use_is_shown_and_the_rest_carry_on() {
        let link = Link::spawn(fake_game("pose|broken|$"));
        link.set_open(true);
        let s = wait_for(&link, "a fault", |l| !l.faults.is_empty() && l.world.is_some());
        assert_eq!(s.faults[0].0, "Pose");
        assert!(s.faults[0].1.contains("pose"), "{:?}", s.faults);
    }

    #[test]
    fn a_closed_port_is_off_and_the_rail_says_so() {
        let closed = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().to_string()
        };
        let link = Link::spawn(closed);
        link.set_open(true);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(link.snapshot().state, LinkState::Off);
        assert!(!link.up());
    }

    /// The page does not lock a REPL user out: the host serves one client at a time, as the bridge does,
    /// and a second one is answered once the session rotates.
    #[test]
    fn a_waiting_client_gets_a_turn_within_a_session() {
        let addr = fake_game("pose|1 2 3 4 5 6|-|-|-|$");
        let link = Link::spawn(addr.clone());
        link.set_open(true);
        wait_for(&link, "streaming", |l| l.state == LinkState::Streaming);
        let asked = Instant::now();
        let mut repl = Bridge::connect(addr.as_str(), MIN_SEND, SESSION + Duration::from_secs(2)).expect("connect");
        let answer = repl.eval("-- live:mission\nreturn 1").expect("a turn");
        assert!(answer.contains("mission"), "{answer}");
        assert!(asked.elapsed() <= SESSION + Duration::from_secs(1), "waited {:?}", asked.elapsed());
    }

    #[test]
    fn a_recording_plays_back_through_the_same_path() {
        let path = std::env::temp_dir().join(format!("live_link_{}.replay", std::process::id()));
        let link = Link::spawn(fake_game("pose|1 2 3 4 5 6|-|-|-|$"));
        link.set_open(true);
        wait_for(&link, "streaming", |l| l.state == LinkState::Streaming);
        link.record_to(path.clone());
        wait_for(&link, "recording", |l| l.recording.is_some());
        std::thread::sleep(Duration::from_millis(400));
        link.stop_recording();
        wait_for(&link, "stopped", |l| l.recording.is_none());
        link.set_open(false);

        let replayed = Link::spawn("127.0.0.1:1".to_string());
        replayed.replay(path.clone());
        let s = wait_for(&replayed, "the end of the replay", |l| matches!(l.state, LinkState::Replay { finished: true, .. }));
        assert_eq!(s.poses.last().expect("a replayed pose").1.pos, [1.0, 2.0, 3.0]);
        assert!(!replayed.up(), "a replay is not a game");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_broken_recording_names_its_line() {
        let path = std::env::temp_dir().join(format!("live_link_bad_{}.replay", std::process::id()));
        std::fs::write(&path, "not json\n").unwrap();
        let link = Link::spawn("127.0.0.1:1".to_string());
        link.replay(path.clone());
        let s = wait_for(&link, "failure", |l| matches!(l.state, LinkState::Failed(_)));
        let LinkState::Failed(e) = s.state else { unreachable!() };
        assert!(e.contains(":1:"), "{e}");
        std::fs::remove_file(&path).unwrap();
    }
}
