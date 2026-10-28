//! Data-driven human animation selection — the engine's real clip picker.
//!
//! The retail engine never hardcodes a character's clip hashes. It selects them
//! through the resident `animationtable` assets (type `0x207359C7`):
//!
//! ```text
//! game state ─(ActionTable 0x6802C321)→ Handle
//!            ─(AnimationLookup 0xE00B080C, scored against the character's key)→ Animation cell
//!            → ASTO list [count][clips] → one clip, drawn from the game's random state
//! ```
//!
//! The key is the character's template identity (`HumanAnimationSet`: `Gender` + `CharacterName`,
//! [`AnimIdentity`]) plus the equipment it carries ([`LookupKey`]); [`AnimSelector::query`] is
//! retail's `FUN_00743CD0` query, rule by rule. The `Animation` cell is an index into the
//! lookup's `ASTO` pool that points at a **list** — `[count][count clip hashes]` — and retail draws
//! one of them per query ([`LookupRow::clip_at`]). Layout + live proof: [`AnimTable`].
//!
//! This module parses the `AnimationLookup` and the `ActionTable`.

use std::collections::BTreeMap;

use crate::hash::pandemic_hash_m2;
use crate::worldentity::{Payload, WorldEntity};

/// `pandemic_hash_m2("animationtable")` — ASET type of every table below.
pub const TYPE_ANIMTABLE: u32 = 0x2073_59C7;
/// The AnimationLookup's asset name hash — (Handle, Gender, CharacterName, Equipment) → Animation list.
/// Its name is not known: `pandemic_hash_m2("AnimationLookup")` is `0x2987300B`.
pub const ANIMLOOKUP: u32 = 0xE00B_080C;
/// `pandemic_hash_m2("ActionTable")` — game state (Stance/Action/AimState/…) → AnimationHandles.
pub const ACTIONTABLE: u32 = 0x6802_C321;
/// The tables' none-sentinel (`0x27DE7135`, `"*"`) — "any"/unset in key columns.
pub const NONE_SENTINEL: u32 = 0x27DE_7135;
/// `pandemic_hash_m2("TRUE")` — the ActionTable's true flag value.
pub const FLAG_TRUE: u32 = 0x1E52_08DD;
/// The second ActionTable `Driven` value retail treats as driven (33 rows; name not recovered).
pub const DRIVEN_ALT: u32 = 0x14BD_1BBD;
/// `pandemic_hash_m2("Bare")` — the `PrimaryEquipmentClass` of an unarmed character.
pub const EQUIPMENT_BARE: u32 = 0xD20E_C345;
/// The Upright idle-cluster head Handle (retail's Upright locomotion list `0x00CFFE58`).
#[cfg(test)]
const PRIMARY_IDLE_HANDLE: u32 = 0x700D_4DE0;

fn r_u16(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}
fn r_u32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

/// A parsed `animationtable` container, laid out the way retail's loader `FUN_0067C780` holds it:
///
/// - `VALU` row 0 is the **defaults row** (table `+0x2C`/`+0x30`), not data. The query scores a cell
///   equal to it as a wildcard hit and the key starts as a copy of it. The data rows are `VALU` rows
///   `1..count` (table `+0x1C` = `count - 1`; live AnimationLookup: `0x5CC` = 1484 of 1485).
/// - `ASTO` is `[u32 n][n u32 pool]`; retail keeps the pool (`+0x50`). An index-typed cell (the
///   AnimationLookup's `Animation`) is a pool index that points at a list `[count][count clips]`
///   (`FUN_00743CD0`: `edi = [cell]`, clip = `[cell + 4 + k*4]`). PROVEN live: row 718's cell
///   `0x5B9` is `pool + 0x5B9*4`, which holds `[1, 0x937D421D]`.
#[derive(Debug)]
pub struct AnimTable {
    cols: Vec<String>,
    key_dims: usize,
    total_dims: usize,
    defaults: Vec<u32>,
    rows: Vec<u32>, // flat: data rows * total_dims
    pool: Vec<u32>,
}

impl AnimTable {
    fn col(&self, name: &str) -> Option<usize> {
        self.cols.iter().position(|c| c.eq_ignore_ascii_case(name))
    }
    /// Column names, in `TYPE` order.
    pub fn cols(&self) -> &[String] {
        &self.cols
    }
    /// `INFO` keyDims.
    pub fn key_dims(&self) -> usize {
        self.key_dims
    }
    /// The defaults row (`VALU` row 0).
    pub fn defaults(&self) -> &[u32] {
        &self.defaults
    }
    /// Number of data rows (`INFO` count − 1).
    pub fn row_count(&self) -> usize {
        self.rows.len() / self.total_dims
    }
    /// Data row `i` (`VALU` row `i + 1`), every column verbatim.
    pub fn row(&self, i: usize) -> &[u32] {
        &self.rows[i * self.total_dims..(i + 1) * self.total_dims]
    }
    /// Every data row, in table order.
    pub fn data_rows(&self) -> impl Iterator<Item = &[u32]> + '_ {
        self.rows.chunks_exact(self.total_dims)
    }
    /// The `ASTO` pool (empty when the container has none).
    pub fn pool(&self) -> &[u32] {
        &self.pool
    }
    /// The list an index-typed cell points at: `pool[idx]` clips from `pool[idx + 1]`. An error when
    /// the cell or its list runs past the pool.
    pub fn list(&self, idx: u32) -> Result<&[u32], String> {
        let i = idx as usize;
        let n = *self.pool.get(i).ok_or_else(|| format!("ASTO index {idx} is past the {}-word pool", self.pool.len()))?
            as usize;
        self.pool
            .get(i + 1..i + 1 + n)
            .ok_or_else(|| format!("ASTO list at {idx} (count {n}) runs past the {}-word pool", self.pool.len()))
    }

    /// Parse a UCFX `animationtable` container (`INFO → TYPE → [ASTO] → VALU`).
    pub fn parse(cont: &[u8]) -> Result<AnimTable, String> {
        if cont.len() < 20 || &cont[0..4] != b"UCFX" {
            return Err("not a UCFX container".into());
        }
        let data_area = r_u32(cont, 4) as usize;
        let ndesc = r_u32(cont, 16) as usize;
        if ndesc == 0 || ndesc > 64 || 20 + ndesc * 20 > cont.len() {
            return Err(format!("{ndesc} chunk descriptors do not fit the {}-byte container", cont.len()));
        }
        let (mut key_dims, mut total_dims, mut count) = (0usize, 0usize, 0usize);
        let (mut type_body, mut valu_body, mut asto_body) = (None, None, None);
        for i in 0..ndesc {
            let off = 20 + i * 20;
            let tag = &cont[off..off + 4];
            let body_off = r_u32(cont, off + 4) as usize;
            let body_sz = r_u32(cont, off + 8) as usize;
            let start = data_area + body_off;
            let body = cont.get(start..start + body_sz).ok_or_else(|| {
                format!("chunk {} at {start}+{body_sz} runs past the {}-byte container", String::from_utf8_lossy(tag), cont.len())
            })?;
            match tag {
                b"INFO" if body_sz >= 6 => {
                    key_dims = r_u16(body, 0) as usize;
                    total_dims = r_u16(body, 2) as usize;
                    count = r_u16(body, 4) as usize;
                }
                b"TYPE" => type_body = Some(body),
                b"VALU" => valu_body = Some(body),
                b"ASTO" => asto_body = Some(body),
                _ => {}
            }
        }
        if total_dims == 0 || count == 0 {
            return Err(format!("INFO: totalDims {total_dims}, count {count}"));
        }
        // TYPE = total_dims × ([ASCII name]\0 [u16 field]).
        let cols = {
            let tb = type_body.ok_or("no TYPE chunk")?;
            let mut names = Vec::with_capacity(total_dims);
            let mut p = 0usize;
            for _ in 0..total_dims {
                let s = p;
                while p < tb.len() && tb[p] != 0 {
                    p += 1;
                }
                names.push(String::from_utf8_lossy(&tb[s..p]).into_owned());
                p += 1 + 2; // NUL + trailing u16
                if p > tb.len() {
                    break;
                }
            }
            names
        };
        if cols.len() != total_dims {
            return Err(format!("TYPE names {} columns, INFO says {total_dims}", cols.len()));
        }
        // VALU = count × total_dims u32: the defaults row, then the data rows.
        let vb = valu_body.ok_or("no VALU chunk")?;
        if vb.len() != count * total_dims * 4 {
            return Err(format!("VALU is {} bytes, INFO says {count} rows × {total_dims} u32", vb.len()));
        }
        let mut values: Vec<u32> = (0..count * total_dims).map(|i| r_u32(vb, i * 4)).collect();
        let rows = values.split_off(total_dims);
        // ASTO = [u32 n][n u32 pool].
        let pool: Vec<u32> = match asto_body {
            None => Vec::new(),
            Some(ab) => {
                let words: Vec<u32> = (0..ab.len() / 4).map(|i| r_u32(ab, i * 4)).collect();
                match words.split_first() {
                    Some((&n, pool)) if ab.len() % 4 == 0 && n as usize == pool.len() => pool.to_vec(),
                    _ => return Err(format!("ASTO ({} bytes) is not [u32 n][n u32]", ab.len())),
                }
            }
        };
        Ok(AnimTable {
            cols,
            key_dims,
            total_dims,
            defaults: values,
            rows,
            pool,
        })
    }
}

/// The AnimationLookup's columns in the order retail reads them — by position, not by name: the key
/// builder (`0x00743B35`) writes cells 1–6 at fixed offsets and `FUN_00743CD0` reads 7–9.
const LOOKUP_COLUMNS: [&str; 10] = [
    "Handle",
    "Gender",
    "CharacterName",
    "PrimaryEquipmentClass",
    "PrimaryEquipmentName",
    "InUseEquipmentClass",
    "InUseEquipmentName",
    "Animation",
    "MinTimeScale",
    "MaxTimeScale",
];
/// How many leading columns `FUN_00743CD0` scores (`push 7` to the query `FUN_0067CF20`).
const LOOKUP_SCORED: usize = 7;
/// The column a candidate must match or leave at the default (`PrimaryEquipmentClass`; the loader
/// sets table `+0x54 = 3` for the AnimationLookup, `FUN_0067C780`).
const LOOKUP_FILTER: usize = 3;
const COL_CHARACTER: usize = 2;
const COL_ANIMATION: usize = 7;
const COL_MIN_TIME_SCALE: usize = 8;
const COL_MAX_TIME_SCALE: usize = 9;

/// Column indices of the ActionTable (resolved by TYPE name, not position).
#[derive(Debug)]
struct ActCols {
    stance: usize,
    action: usize,
    aim_state: usize,
    tandem: usize,
    seat: usize,
    target: usize,
    action_direction: usize,
    damage_direction: usize,
    animation_handles: usize,
    partition_mask: usize,
    looping: usize,
    driven: usize,
    action_mask: usize,
    locomotion_mask: usize,
}

/// Resolves a character's clips through the resident AnimationLookup.
#[derive(Debug)]
pub struct AnimSelector {
    lookup: AnimTable,
    /// The ActionTable (same resident block) + its column indices, when present.
    actions: Option<(AnimTable, ActCols)>,
}

impl AnimSelector {
    /// Build from a decompressed resident block. `None` when the block does not hold the
    /// AnimationLookup (`0xE00B080C`); an error when it holds a table that does not parse.
    pub fn from_resident_block(dec: &[u8]) -> Result<Option<AnimSelector>, String> {
        let Some(lookup) = find_container(dec, ANIMLOOKUP) else {
            return Ok(None);
        };
        let actions = find_container(dec, ACTIONTABLE);
        Self::from_containers(&lookup, actions.as_deref()).map(Some)
    }

    /// Build from the two `animationtable` containers themselves — the AnimationLookup
    /// (`0xE00B080C`) and, when present, the ActionTable (`0x6802C321`) — however the caller
    /// resolved them (by asset name through the WAD stack, not by block number). An error when the
    /// lookup's columns are not retail's, when any row's `Animation` list runs past the pool, or when
    /// the ActionTable lacks a column.
    pub fn from_containers(lookup: &[u8], action_table: Option<&[u8]>) -> Result<AnimSelector, String> {
        let lookup = AnimTable::parse(lookup).map_err(|e| format!("AnimationLookup: {e}"))?;
        if lookup.cols().iter().map(String::as_str).ne(LOOKUP_COLUMNS) {
            return Err(format!("AnimationLookup: columns {:?}, retail reads {LOOKUP_COLUMNS:?}", lookup.cols()));
        }
        for row in std::iter::once(lookup.defaults()).chain(lookup.data_rows()) {
            lookup.list(row[COL_ANIMATION]).map_err(|e| format!("AnimationLookup row {row:08X?}: {e}"))?;
        }
        let actions = match action_table {
            None => None,
            Some(at) => {
                let t = AnimTable::parse(at).map_err(|e| format!("ActionTable: {e}"))?;
                let col = |n: &str| t.col(n).ok_or_else(|| format!("ActionTable: no column {n:?}"));
                let cols = ActCols {
                    stance: col("Stance")?,
                    action: col("Action")?,
                    aim_state: col("AimState")?,
                    tandem: col("Tandem")?,
                    seat: col("Seat")?,
                    target: col("Target")?,
                    action_direction: col("ActionDirection")?,
                    damage_direction: col("DamageDirection")?,
                    animation_handles: col("AnimationHandles")?,
                    partition_mask: col("PartitionMask")?,
                    looping: col("Looping")?,
                    driven: col("Driven")?,
                    action_mask: col("ActionMask")?,
                    locomotion_mask: col("LocomotionMask")?,
                };
                Some((t, cols))
            }
        };
        Ok(AnimSelector { lookup, actions })
    }

    /// `CharacterName` hash for a merc — `pandemic_hash_m2(name)` ("mattias"/"chris"/"jennifer").
    pub fn character_name(merc: &str) -> u32 {
        pandemic_hash_m2(merc)
    }

    /// The clips of a lookup row's `Animation` list (validated when the selector was built).
    fn clips_of(&self, row: &[u32]) -> &[u32] {
        self.lookup.list(row[COL_ANIMATION]).expect("every Animation list is checked in from_containers")
    }

    /// Every `(CharacterName, clip)` the AnimationLookup keys to `handle` — every clip of every row's
    /// list — for probing which character key a shared animation resolves under.
    pub fn handle_clips(&self, handle: u32) -> Vec<(u32, u32)> {
        self.lookup
            .data_rows()
            .filter(|r| r[0] == handle)
            .flat_map(|r| self.clips_of(r).iter().map(move |&c| (r[COL_CHARACTER], c)))
            .collect()
    }

    /// The AnimationHandles the ActionTable assigns to a `(stance, action)` state key (there may be
    /// several rows). Empty when no ActionTable is loaded or the state is absent.
    pub fn handles_for_state(&self, stance: u32, action: u32) -> Vec<u32> {
        let Some((t, c)) = self.actions.as_ref() else {
            return Vec::new();
        };
        t.data_rows()
            .filter(|r| r[c.stance] == stance && r[c.action] == action)
            .map(|r| r[c.animation_handles])
            .collect()
    }

    /// Every distinct `(Stance, Action)` pair present in the ActionTable — for probing the state
    /// vocabulary (reverse the hashes via the rainbow table). Empty when no ActionTable is loaded.
    pub fn action_states(&self) -> Vec<(u32, u32)> {
        let Some((t, c)) = self.actions.as_ref() else {
            return Vec::new();
        };
        let seen: std::collections::BTreeSet<(u32, u32)> = t.data_rows().map(|r| (r[c.stance], r[c.action])).collect();
        seen.into_iter().collect()
    }

    /// Every clip of every AnimationLookup row keyed to `character`, in table order — the
    /// character's own animation set (equipment/gender variant rows share a Handle and list their own
    /// clips). Empty when the character has no personal rows.
    pub fn character_clips(&self, character: u32) -> Vec<CharacterClip> {
        self.lookup
            .data_rows()
            .filter(|r| r[COL_CHARACTER] == character)
            .flat_map(|r| self.clips_of(r).iter().map(move |&clip| CharacterClip { handle: r[0], clip }))
            .collect()
    }

    /// All ActionTable rows whose `AnimationHandles` is `handle` — the game states (Stance/
    /// Action/AimState/ActionDirection/…) that play this handle. Empty if the block carried no
    /// ActionTable.
    pub fn handle_actions(&self, handle: u32) -> Vec<ActionRow> {
        let Some((t, c)) = &self.actions else {
            return Vec::new();
        };
        t.data_rows()
            .filter(|r| r[c.animation_handles] == handle)
            .map(|r| ActionRow {
                stance: r[c.stance],
                action: r[c.action],
                aim_state: r[c.aim_state],
                tandem: r[c.tandem],
                seat: r[c.seat],
                target: r[c.target],
                action_direction: r[c.action_direction],
                damage_direction: r[c.damage_direction],
                partition_mask: r[c.partition_mask],
                looping: r[c.looping],
                driven: r[c.driven],
                action_mask: r[c.action_mask],
                locomotion_mask: r[c.locomotion_mask],
            })
            .collect()
    }

    /// Retail's AnimationLookup query (`FUN_00743CD0` → `FUN_0067CF20` → `FUN_0067CBC0` /
    /// `FUN_0067C5D0`), rule by rule:
    ///
    /// 1. The key starts as a copy of the defaults row (`0x0055BFE0`); `handle` and every set
    ///    [`LookupKey`] cell overwrite it (`0x00743B35`).
    /// 2. Candidates are the data rows whose `Handle` is `handle`, in table order (the Handle index's
    ///    posting list — read live, ascending) whose `PrimaryEquipmentClass` is the key's or the
    ///    default.
    /// 3. Each candidate scores, over the first 7 columns, 2 per cell equal to the key and 1 per cell
    ///    equal to the defaults row; any other cell scores 0 and does not disqualify.
    /// 4. The first candidate with the strictly highest score wins.
    /// 5. The winner is rejected unless its `CharacterName` is the key's or the default.
    ///
    /// `None` when no row survives — retail then plays its placeholder `0x9D886544`.
    pub fn query(&self, handle: u32, key: &LookupKey) -> Option<LookupRow> {
        let d = self.lookup.defaults();
        let mut k = [0u32; LOOKUP_SCORED];
        k.copy_from_slice(&d[..LOOKUP_SCORED]);
        k[0] = handle;
        for (c, v) in [
            (1, key.gender),
            (2, key.character),
            (3, key.primary_equipment_class),
            (4, key.primary_equipment_name),
            (5, key.in_use_equipment_class),
            (6, key.in_use_equipment_name),
        ] {
            if v != 0 {
                k[c] = v;
            }
        }
        let mut best: Option<(&[u32], u32)> = None;
        for r in self.lookup.data_rows() {
            if r[0] != handle || (r[LOOKUP_FILTER] != k[LOOKUP_FILTER] && r[LOOKUP_FILTER] != d[LOOKUP_FILTER]) {
                continue;
            }
            let score = (0..LOOKUP_SCORED)
                .map(|c| if r[c] == k[c] { 2 } else if r[c] == d[c] { 1 } else { 0 })
                .sum();
            if best.map_or(true, |(_, s)| score > s) {
                best = Some((r, score));
            }
        }
        let (r, _) = best?;
        if r[COL_CHARACTER] != k[COL_CHARACTER] && r[COL_CHARACTER] != d[COL_CHARACTER] {
            return None;
        }
        Some(self.lookup_row(r))
    }

    fn lookup_row(&self, r: &[u32]) -> LookupRow {
        LookupRow {
            handle: r[0],
            gender: r[1],
            character: r[2],
            primary_equipment_class: r[3],
            primary_equipment_name: r[4],
            in_use_equipment_class: r[5],
            in_use_equipment_name: r[6],
            clips: self.clips_of(r).to_vec(),
            min_time_scale: f32::from_bits(r[COL_MIN_TIME_SCALE]),
            max_time_scale: f32::from_bits(r[COL_MAX_TIME_SCALE]),
        }
    }

    /// Every AnimationLookup row keyed to `handle`, all characters, in table order.
    pub fn lookup_rows(&self, handle: u32) -> Vec<LookupRow> {
        self.lookup.data_rows().filter(|r| r[0] == handle).map(|r| self.lookup_row(r)).collect()
    }

    /// One entry per clip of every AnimationLookup row for (`handle`, `character`), with the row's
    /// equipment/gender keys and timescale range — how the SAME Handle resolves to different clips
    /// per loadout.
    pub fn lookup_context(&self, handle: u32, character: u32) -> Vec<LookupContext> {
        self.lookup
            .data_rows()
            .filter(|r| r[0] == handle && r[COL_CHARACTER] == character)
            .flat_map(|r| {
                self.clips_of(r).iter().map(move |&clip| LookupContext {
                    handle,
                    gender: r[1],
                    primary_equipment_class: r[3],
                    primary_equipment_name: r[4],
                    in_use_equipment_class: r[5],
                    in_use_equipment_name: r[6],
                    clip,
                    min_time_scale: f32::from_bits(r[COL_MIN_TIME_SCALE]),
                    max_time_scale: f32::from_bits(r[COL_MAX_TIME_SCALE]),
                })
            })
            .collect()
    }
}

/// What an AnimationLookup query is keyed on, for one character at one moment: its template
/// identity ([`AnimIdentity`]) plus what it carries. `0` = not set: that cell keeps the defaults
/// row's value, as retail's key builder leaves it (it writes `Gender`/`CharacterName` only when
/// non-zero, and the in-use equipment only when the character holds something in use).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LookupKey {
    pub character: u32,
    pub gender: u32,
    pub primary_equipment_class: u32,
    pub primary_equipment_name: u32,
    pub in_use_equipment_class: u32,
    pub in_use_equipment_name: u32,
}

impl LookupKey {
    /// `identity` carrying a primary equipment class and nothing else. Retail keys a character with
    /// no primary equipment as [`EQUIPMENT_BARE`] (`0x00743BC9`/`0x00743BFC`).
    pub fn new(identity: AnimIdentity, primary_equipment_class: u32) -> LookupKey {
        LookupKey {
            character: identity.character,
            gender: identity.gender,
            primary_equipment_class,
            primary_equipment_name: 0,
            in_use_equipment_class: 0,
            in_use_equipment_name: 0,
        }
    }
}

/// A template's animation identity — its `HumanAnimationSet` component: the `Gender` and
/// `CharacterName` the AnimationLookup is keyed on (mercs: `Hero` `0x51728909` + `mattias` /
/// `chris` / `jennifer`). Read live in a retail capture as `{u32 gender, u32 character}` per
/// template (container `0x017BCD18`, stride 8). `0` = unset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimIdentity {
    pub gender: u32,
    pub character: u32,
}

/// Every template's `HumanAnimationSet`, by template key. An error when a payload is not two u32s
/// or a key carries two different values.
pub fn human_animation_sets(we: &WorldEntity) -> Result<BTreeMap<u32, AnimIdentity>, String> {
    let mut out: BTreeMap<u32, AnimIdentity> = BTreeMap::new();
    for c in we.groups_of("HumanAnimationSet") {
        for r in &c.records {
            let id = match &r.payload {
                Payload::Fixed(b) if b.len() == 8 => AnimIdentity { gender: r_u32(b, 0), character: r_u32(b, 4) },
                p => return Err(format!("HumanAnimationSet: payload {p:?} is not two u32")),
            };
            for &k in &r.keys {
                if let Some(old) = out.insert(k, id) {
                    if old != id {
                        return Err(format!("HumanAnimationSet: key 0x{k:08X} has {old:?} and {id:?}"));
                    }
                }
            }
        }
    }
    Ok(out)
}

/// One ActionTable row (all values verbatim from the table; key columns use
/// [`NONE_SENTINEL`] for "any").
#[derive(Clone, Copy, Debug)]
pub struct ActionRow {
    pub stance: u32,
    pub action: u32,
    pub aim_state: u32,
    pub tandem: u32,
    pub seat: u32,
    pub target: u32,
    pub action_direction: u32,
    pub damage_direction: u32,
    pub partition_mask: u32,
    pub looping: u32,
    pub driven: u32,
    pub action_mask: u32,
    pub locomotion_mask: u32,
}

impl ActionRow {
    /// Whether the clip loops. Retail loops only on an exact [`FLAG_TRUE`]; `FALSE` and `"*"` both
    /// play once (`FUN_0068A8C0`: `row+0x28 == 0x1E5208DD` → play mode 3, else 2).
    pub fn loops(&self) -> bool {
        self.looping == FLAG_TRUE
    }

    /// Whether the clip drives the root (root motion). Retail treats [`FLAG_TRUE`] and
    /// [`DRIVEN_ALT`] as driven and anything else — `"*"` included — as not (`FUN_0068A4B0`:
    /// `row+0x2C == 0x1E5208DD || == 0x14BD1BBD`).
    pub fn is_driven(&self) -> bool {
        self.driven == FLAG_TRUE || self.driven == DRIVEN_ALT
    }
}

/// One clip of an AnimationLookup row, with the row's equipment context: which loadout the
/// (Handle, CharacterName) pair can play `clip` under. `min/max_time_scale` = allowed playback-rate
/// range (`-1.0` = default).
#[derive(Clone, Copy, Debug)]
pub struct LookupContext {
    pub handle: u32,
    pub gender: u32,
    pub primary_equipment_class: u32,
    pub primary_equipment_name: u32,
    pub in_use_equipment_class: u32,
    pub in_use_equipment_name: u32,
    pub clip: u32,
    pub min_time_scale: f32,
    pub max_time_scale: f32,
}

/// One AnimationLookup row with every key column verbatim ([`NONE_SENTINEL`] = "*", any) and the
/// clips of its `Animation` list.
#[derive(Clone, Debug, PartialEq)]
pub struct LookupRow {
    pub handle: u32,
    pub character: u32,
    pub gender: u32,
    pub primary_equipment_class: u32,
    pub primary_equipment_name: u32,
    pub in_use_equipment_class: u32,
    pub in_use_equipment_name: u32,
    pub clips: Vec<u32>,
    pub min_time_scale: f32,
    pub max_time_scale: f32,
}

impl LookupRow {
    /// The clip retail plays for one draw from the game's random state: `clips[draw % count]`, where
    /// `draw` is the generator's two-step value `(x & 0xFFFF0000) | (u >> 16)` (`FUN_00743CD0`
    /// `0x00743D25`–`0x00743D5B`).
    pub fn clip_at(&self, draw: u32) -> u32 {
        self.clips[(draw % self.clips.len() as u32) as usize]
    }
}

/// One clip an AnimationLookup row lists for a character: the logical Handle (the ActionTable's
/// `AnimationHandles` join key) and the clip hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CharacterClip {
    pub handle: u32,
    pub clip: u32,
}

/// Find a UCFX asset container by its entry-table `name_hash` in a multi-entry block.
fn find_container(dec: &[u8], name_hash: u32) -> Option<Vec<u8>> {
    if dec.len() < 4 {
        return None;
    }
    let count = r_u32(dec, 0) as usize;
    let max = dec.len().saturating_sub(4) / 16;
    let count = count.min(max);
    let mut pos = 4 + count * 16;
    for i in 0..count {
        let b = 4 + i * 16;
        let nh = r_u32(dec, b);
        let sz = r_u32(dec, b + 12) as usize;
        if pos + sz > dec.len() {
            break;
        }
        if nh == name_hash {
            return Some(dec[pos..pos + sz].to_vec());
        }
        pos += sz;
    }
    None
}

/// True if `dec` is the resident block holding the AnimationLookup.
pub fn block_has_lookup(dec: &[u8]) -> bool {
    find_container(dec, ANIMLOOKUP).is_some()
}

/// True if a block entry table contains a type-`0x207359C7` animationtable (cheap probe).
pub fn block_has_animtable(dec: &[u8]) -> bool {
    if dec.len() < 4 {
        return false;
    }
    let count = (r_u32(dec, 0) as usize).min(dec.len().saturating_sub(4) / 16);
    (0..count).any(|i| r_u32(dec, 4 + i * 16 + 4) == TYPE_ANIMTABLE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p32(b: &mut Vec<u8>, v: u32) {
        b.extend_from_slice(&v.to_le_bytes());
    }

    /// Build one UCFX animationtable container: `INFO → TYPE → [ASTO] → VALU`. `rows[0]` is the
    /// defaults row; `pool` is written as `ASTO = [len][pool]`.
    fn synth_table(cols: &[&str], rows: &[&[u32]], pool: &[u32]) -> Vec<u8> {
        let mut info = Vec::new();
        info.extend_from_slice(&(cols.len() as u16).to_le_bytes()); // keyDims
        info.extend_from_slice(&(cols.len() as u16).to_le_bytes()); // totalDims
        info.extend_from_slice(&(rows.len() as u16).to_le_bytes()); // count
        let mut typ = Vec::new();
        for name in cols {
            typ.extend_from_slice(name.as_bytes());
            typ.push(0);
            typ.extend_from_slice(&0u16.to_le_bytes());
        }
        let mut asto = Vec::new();
        p32(&mut asto, pool.len() as u32);
        for &v in pool {
            p32(&mut asto, v);
        }
        let mut valu = Vec::new();
        for row in rows {
            assert_eq!(row.len(), cols.len());
            for &v in *row {
                p32(&mut valu, v);
            }
        }

        let mut bodies: Vec<(&[u8; 4], &Vec<u8>)> = vec![(b"INFO", &info), (b"TYPE", &typ)];
        if !pool.is_empty() {
            bodies.push((b"ASTO", &asto));
        }
        bodies.push((b"VALU", &valu));
        let ndesc = bodies.len();
        let data_area = 20 + ndesc * 20;
        let mut cont = Vec::new();
        cont.extend_from_slice(b"UCFX");
        p32(&mut cont, data_area as u32); // +4 data_area
        p32(&mut cont, 0);
        p32(&mut cont, 0); // +8,+12 unused
        p32(&mut cont, ndesc as u32); // +16 ndesc
        let mut off = 0u32;
        for (tag, body) in &bodies {
            cont.extend_from_slice(*tag);
            p32(&mut cont, off);
            p32(&mut cont, body.len() as u32);
            cont.extend_from_slice(&[0u8; 8]);
            off += body.len() as u32;
        }
        for (_, body) in &bodies {
            cont.extend_from_slice(body);
        }
        cont
    }

    /// Wrap named containers into a resident-block entry table.
    fn synth_resident(entries: &[(u32, &Vec<u8>)]) -> Vec<u8> {
        let mut block = Vec::new();
        p32(&mut block, entries.len() as u32);
        for (nh, cont) in entries {
            p32(&mut block, *nh); // name_hash
            p32(&mut block, TYPE_ANIMTABLE); // type_hash
            p32(&mut block, 0); // field_c
            p32(&mut block, cont.len() as u32); // size
        }
        for (_, cont) in entries {
            block.extend_from_slice(cont);
        }
        block
    }

    const N: u32 = NONE_SENTINEL;
    const HERO: u32 = 0x5172_8909;
    const MATTIAS: u32 = 0x030E_6C38;
    const RIFLE: u32 = 0xD045_9A41;
    const TS: u32 = 0xBF80_0000; // -1.0

    /// A lookup row `(handle, gender, character, primary class, *, *, *, list, -1, -1)`.
    fn lk_row(h: u32, g: u32, c: u32, pec: u32, list: u32) -> [u32; 10] {
        [h, g, c, pec, N, N, N, list, TS, TS]
    }

    /// Retail's ASTO is a run of `[count][clips]` lists; the parser keeps the pool after the length
    /// word and a cell resolves to its whole list.
    #[test]
    fn animation_cells_point_at_count_prefixed_lists() {
        let defaults = lk_row(N, N, N, N, 0);
        let one = lk_row(PRIMARY_IDLE_HANDLE, HERO, MATTIAS, EQUIPMENT_BARE, 2);
        let two = lk_row(PRIMARY_IDLE_HANDLE, HERO, MATTIAS, RIFLE, 4);
        let pool = [1, N, 1, 0xAAAA, 2, 0xBBBB, 0xCCCC];
        let t = AnimTable::parse(&synth_table(&LOOKUP_COLUMNS, &[&defaults, &one, &two], &pool)).unwrap();
        assert_eq!(t.row_count(), 2, "VALU row 0 is the defaults row, not data");
        assert_eq!(t.defaults(), &defaults);
        assert_eq!(t.list(0).unwrap(), &[N]);
        assert_eq!(t.list(2).unwrap(), &[0xAAAA]);
        assert_eq!(t.list(4).unwrap(), &[0xBBBB, 0xCCCC]);
        assert!(t.list(5).is_err(), "0xBBBB as a count runs past the pool");

        let sel = AnimSelector::from_containers(&synth_table(&LOOKUP_COLUMNS, &[&defaults, &one, &two], &pool), None)
            .unwrap();
        let row = sel.query(PRIMARY_IDLE_HANDLE, &LookupKey::new(AnimIdentity { gender: HERO, character: MATTIAS }, RIFLE));
        let row = row.expect("the Rifle row");
        assert_eq!(row.clips, vec![0xBBBB, 0xCCCC]);
        assert_eq!(row.clip_at(0), 0xBBBB);
        assert_eq!(row.clip_at(7), 0xCCCC);
    }

    /// An `ASTO` whose length word disagrees with its body, and a row whose list runs past the
    /// pool, are rejected — never read short.
    #[test]
    fn malformed_pools_are_errors() {
        let defaults = lk_row(N, N, N, N, 0);
        let mut bad_len = synth_table(&LOOKUP_COLUMNS, &[&defaults], &[1, N]);
        // ASTO is the third chunk; its body starts right after the 3 + 1 descriptors' data offset.
        let asto_at = 20 + 4 * 20 + 6 + LOOKUP_COLUMNS.iter().map(|c| c.len() + 3).sum::<usize>();
        bad_len[asto_at] = 9;
        assert!(AnimTable::parse(&bad_len).unwrap_err().contains("ASTO"));
        let past = lk_row(PRIMARY_IDLE_HANDLE, N, N, N, 1);
        let err = AnimSelector::from_containers(&synth_table(&LOOKUP_COLUMNS, &[&defaults, &past], &[1, N]), None);
        assert!(err.is_err());
    }

    /// Retail scoring: 2 per cell equal to the key, 1 per cell at the default, first strict max
    /// wins, `PrimaryEquipmentClass` filters, `CharacterName` must match or be the default.
    #[test]
    fn query_scores_like_retail() {
        const H: u32 = 0xB99A_D3B3;
        const NPC: u32 = 0x1111_1111;
        const OTHER: u32 = 0x4444_4444;
        let pool = [1, N, 1, 0xA0, 1, 0xA1, 1, 0xA2, 1, 0xA3, 1, 0xA4, 1, 0xA5];
        let rows: [[u32; 10]; 7] = [
            lk_row(N, N, N, N, 0),
            lk_row(H, N, N, EQUIPMENT_BARE, 2),          // shared Bare walk
            lk_row(H, HERO, MATTIAS, EQUIPMENT_BARE, 4), // mattias Bare walk
            lk_row(H, HERO, MATTIAS, RIFLE, 6),          // mattias Rifle walk
            lk_row(H, HERO, NPC, EQUIPMENT_BARE, 8),     // variant A
            lk_row(H, HERO, NPC, EQUIPMENT_BARE, 10),    // variant B, same key
            lk_row(H, N, OTHER, RIFLE, 12),              // someone else's Rifle walk
        ];
        let rows: Vec<&[u32]> = rows.iter().map(|r| &r[..]).collect();
        let sel = AnimSelector::from_containers(&synth_table(&LOOKUP_COLUMNS, &rows, &pool), None).unwrap();
        let clip = |id: AnimIdentity, eq: u32| sel.query(H, &LookupKey::new(id, eq)).map(|r| r.clips[0]);
        let mattias = AnimIdentity { gender: HERO, character: MATTIAS };
        // Own row: 2·7 = 14 beats the shared row's 2+1+1+2+2+2+2 = 12.
        assert_eq!(clip(mattias, EQUIPMENT_BARE), Some(0xA1));
        assert_eq!(clip(mattias, RIFLE), Some(0xA2));
        // A Hero with no row of its own: shared (12) and mattias (2+2+0+2+2+2+2 = 12) tie; the
        // earlier row wins.
        assert_eq!(clip(AnimIdentity { gender: HERO, character: 0x2222_2222 }, EQUIPMENT_BARE), Some(0xA0));
        // Equal-key variants: the first in table order.
        assert_eq!(clip(AnimIdentity { gender: HERO, character: NPC }, EQUIPMENT_BARE), Some(0xA3));
        // An unset identity keeps the defaults row's "*" and matches the shared row exactly.
        assert_eq!(clip(AnimIdentity { gender: 0, character: 0 }, EQUIPMENT_BARE), Some(0xA0));
        // No candidate passes the PrimaryEquipmentClass filter.
        assert_eq!(clip(mattias, 0x3333_3333), None);
        // A stranger with a Rifle: the best candidate is mattias's Rifle row (2+2+0+2+2+2+2 = 12 vs
        // OTHER's 2+1+0+2+2+2+2 = 11), and its CharacterName rejects it.
        assert_eq!(clip(AnimIdentity { gender: HERO, character: 0x2222_2222 }, RIFLE), None);
        // The handle selects the candidates.
        assert_eq!(sel.query(0x5555_5555, &LookupKey::new(mattias, EQUIPMENT_BARE)), None);
    }

    #[test]
    fn action_table_and_lookup_context() {
        let at_cols = [
            "Stance",
            "Action",
            "AimState",
            "Tandem",
            "Seat",
            "Target",
            "ActionDirection",
            "DamageDirection",
            "AnimationHandles",
            "PartitionMask",
            "Looping",
            "Driven",
            "ActionMask",
            "LocomotionMask",
        ];
        let at = synth_table(
            &at_cols,
            &[
                &[N; 14],
                &[
                    0x12C0_7B18, // Stance = Upright (named in the devkit strings)
                    0x0C0A_7FA6, // Action = Fidget
                    N,
                    N,
                    N,
                    N,
                    N,
                    N,
                    PRIMARY_IDLE_HANDLE,
                    0,
                    FLAG_TRUE,
                    0,
                    0,
                    0,
                ],
                // A second row on a DIFFERENT handle must not match.
                &[0x12C0_7B18, 0xDEAD_BEEF, N, N, N, N, N, N, 0x1111_1111, 0, 0, 0, 0, 0],
            ],
            &[],
        );
        let mut lk_mattias = lk_row(PRIMARY_IDLE_HANDLE, N, MATTIAS, 0xCAFE_0001, 2);
        lk_mattias[8] = 0.9f32.to_bits();
        let lk = synth_table(&LOOKUP_COLUMNS, &[&lk_row(N, N, N, N, 0), &lk_mattias], &[1, N, 2, 0xBBBB, 0xCCCC]);
        let block = synth_resident(&[(ANIMLOOKUP, &lk), (ACTIONTABLE, &at)]);
        assert!(block_has_lookup(&block));
        let sel = AnimSelector::from_resident_block(&block).unwrap().expect("block holds the lookup");

        let acts = sel.handle_actions(PRIMARY_IDLE_HANDLE);
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].stance, 0x12C0_7B18);
        assert_eq!(acts[0].action, 0x0C0A_7FA6);
        assert_eq!(acts[0].aim_state, N);
        assert!(acts[0].loops());
        assert!(sel.handle_actions(0x2222_2222).is_empty());
        assert!(sel.handle_actions(N).is_empty(), "the defaults row is not a data row");

        // One context per clip of the row's list.
        let ctx = sel.lookup_context(PRIMARY_IDLE_HANDLE, MATTIAS);
        assert_eq!(ctx.iter().map(|c| c.clip).collect::<Vec<_>>(), vec![0xBBBB, 0xCCCC]);
        assert_eq!(ctx[0].primary_equipment_class, 0xCAFE_0001);
        assert_eq!(ctx[0].min_time_scale, 0.9);
        assert_eq!(ctx[0].max_time_scale, -1.0);
        assert!(sel.lookup_context(PRIMARY_IDLE_HANDLE, 0xF314_4C8E).is_empty());

        assert_eq!(sel.handles_for_state(0x12C0_7B18, 0x0C0A_7FA6), vec![PRIMARY_IDLE_HANDLE]);
        assert!(sel.handles_for_state(0xDEAD_0000, 0x0C0A_7FA6).is_empty());
        assert_eq!(sel.handle_clips(PRIMARY_IDLE_HANDLE), vec![(MATTIAS, 0xBBBB), (MATTIAS, 0xCCCC)]);
        assert_eq!(
            sel.character_clips(MATTIAS),
            vec![
                CharacterClip { handle: PRIMARY_IDLE_HANDLE, clip: 0xBBBB },
                CharacterClip { handle: PRIMARY_IDLE_HANDLE, clip: 0xCCCC }
            ]
        );
    }

    /// The lookup is read by position: a table whose columns are not retail's is refused.
    #[test]
    fn foreign_lookup_columns_are_refused() {
        let cols = ["Handle", "CharacterName", "Animation"];
        let t = synth_table(&cols, &[&[N, N, 0]], &[1, N]);
        assert!(AnimSelector::from_containers(&t, None).unwrap_err().contains("columns"));
    }

    #[test]
    fn character_name() {
        // The keys the engine uses are pandemic_hash_m2 of the merc name.
        assert_eq!(AnimSelector::character_name("mattias"), 0x030E_6C38);
        assert_eq!(AnimSelector::character_name("chris"), 0xD64B_B122);
        assert_eq!(AnimSelector::character_name("jennifer"), 0xF314_4C8E);
    }

    /// Looping/Driven decode as retail reads them: only `TRUE` loops; `TRUE` and the alternate
    /// value drive; `"*"` and `FALSE` do neither.
    #[test]
    fn action_flags_decode_like_retail() {
        let row = |looping, driven| ActionRow {
            stance: 0, action: 0, aim_state: 0, tandem: 0, seat: 0, target: 0, action_direction: 0,
            damage_direction: 0, partition_mask: 0, looping, driven, action_mask: 0, locomotion_mask: 0,
        };
        const FALSE: u32 = 0xCD63_8E76;
        assert!(row(FLAG_TRUE, 0).loops());
        assert!(!row(FALSE, 0).loops());
        assert!(!row(NONE_SENTINEL, 0).loops());
        assert!(row(0, FLAG_TRUE).is_driven());
        assert!(row(0, DRIVEN_ALT).is_driven());
        assert!(!row(0, NONE_SENTINEL).is_driven());
    }
}
