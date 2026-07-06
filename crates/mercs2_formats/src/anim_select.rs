//! Data-driven human animation selection — the engine's real clip picker.
//!
//! The retail engine never hardcodes a character's clip hashes. It selects them
//! through the resident `animationtable` assets (type `0x207359C7`):
//!
//! ```text
//! game state ─(ActionTable 0x6802C321)→ Handle
//!            ─(AnimationLookup 0xE00B080C, keyed by CharacterName)→ Animation index
//!            → ASTO[index] → clip name-hash → the character's animgroup clip
//! ```
//!
//! `CharacterName` is `pandemic_hash_m2(merc)` (mattias/chris/jennifer), which is
//! how one shared table drives each merc's own clips. The `Animation` column is a
//! u32 index into the lookup container's `ASTO` value pool, and `ASTO[index]` is
//! the clip hash. Fully reverse-engineered + validated (against a live x32dbg
//! capture, Chris idle `0xED37BC56`) in
//! `docs/modernization/human_animation_selection.md`.
//!
//! This module parses the `AnimationLookup` and resolves a character's clips by
//! Handle. The base locomotion (walk/run) resolves through a separate default
//! path not modelled here yet; the per-character **idle** — the visible
//! differentiator, and the one the hardcoded engine got wrong (it used Jennifer's
//! `0x24F8C8E6` for everyone) — is resolved here.

use crate::hash::pandemic_hash_m2;

/// `pandemic_hash_m2("animationtable")` — ASET type of every table below.
const TYPE_ANIMTABLE: u32 = 0x2073_59C7;
/// `pandemic_hash_m2("AnimationLookup")` — (Handle, CharacterName, Equipment) → Animation index.
const ANIMLOOKUP: u32 = 0xE00B_080C;
/// `pandemic_hash_m2("ActionTable")` — game state (Stance/Action/AimState/…) → AnimationHandles.
const ACTIONTABLE: u32 = 0x6802_C321;
/// The tables' none-sentinel (`0x27DE7135`) — "any"/unset in key columns.
pub const NONE_SENTINEL: u32 = 0x27DE_7135;
/// The Upright idle-cluster head Handle. Resolving it per CharacterName yields each
/// merc's primary standing idle (validated: mattias `0x6EA88E00`, chris `0x835DA06A`,
/// jennifer `0x24F8C8E6`).
const PRIMARY_IDLE_HANDLE: u32 = 0x700D_4DE0;

fn r_u16(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}
fn r_u32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

/// A parsed `animationtable` container: named columns, `count` rows of `total_dims`
/// u32 each, plus the `ASTO` value pool that index-typed columns point into.
struct AnimTable {
    cols: Vec<String>,
    total_dims: usize,
    rows: Vec<u32>, // flat: count * total_dims
    asto: Vec<u32>,
}

impl AnimTable {
    fn col(&self, name: &str) -> Option<usize> {
        self.cols.iter().position(|c| c.eq_ignore_ascii_case(name))
    }
    fn row_count(&self) -> usize {
        if self.total_dims == 0 { 0 } else { self.rows.len() / self.total_dims }
    }

    /// Parse a UCFX `animationtable` container (`INFO → TYPE → [ASTO] → VALU`).
    fn parse(cont: &[u8]) -> Option<AnimTable> {
        if cont.len() < 20 || &cont[0..4] != b"UCFX" {
            return None;
        }
        let data_area = r_u32(cont, 4) as usize;
        let ndesc = r_u32(cont, 16) as usize;
        if ndesc == 0 || ndesc > 64 || 20 + ndesc * 20 > cont.len() {
            return None;
        }
        let (mut total_dims, mut count) = (0usize, 0usize);
        let (mut type_body, mut valu_body, mut asto_body) = (None, None, None);
        for i in 0..ndesc {
            let off = 20 + i * 20;
            let tag = &cont[off..off + 4];
            let body_off = r_u32(cont, off + 4) as usize;
            let body_sz = r_u32(cont, off + 8) as usize;
            let start = data_area + body_off;
            if start + body_sz > cont.len() {
                continue;
            }
            let body = &cont[start..start + body_sz];
            match tag {
                b"INFO" if body_sz >= 6 => {
                    total_dims = r_u16(body, 2) as usize;
                    count = r_u16(body, 4) as usize;
                }
                b"TYPE" => type_body = Some(body),
                b"VALU" => valu_body = Some(body),
                b"ASTO" => asto_body = Some(body),
                _ => {}
            }
        }
        if total_dims == 0 {
            return None;
        }
        // TYPE = total_dims × ([ASCII name]\0 [u16 field]).
        let cols = {
            let tb = type_body?;
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
        // VALU = count × total_dims u32.
        let vb = valu_body?;
        let n = count.min(vb.len() / 4 / total_dims.max(1)) * total_dims;
        let rows: Vec<u32> = (0..n).map(|i| r_u32(vb, i * 4)).collect();
        // ASTO = value pool of u32 (index-typed columns point in here).
        let asto: Vec<u32> = asto_body
            .map(|ab| (0..ab.len() / 4).map(|i| r_u32(ab, i * 4)).collect())
            .unwrap_or_default();
        Some(AnimTable { cols, total_dims, rows, asto })
    }
}

/// Column indices of the ActionTable (resolved by TYPE name, not position).
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
pub struct AnimSelector {
    lookup: AnimTable,
    h: usize,
    cn: usize,
    an: usize,
    /// Optional equipment/gender/timescale columns of the lookup (present in the retail table).
    lk_ext: Option<(usize, usize, usize, usize, usize, usize, usize)>,
    /// The ActionTable (same resident block) + its column indices, when present.
    actions: Option<(AnimTable, ActCols)>,
}

impl AnimSelector {
    /// Build from a decompressed resident block (the one carrying the
    /// AnimationLookup, `0xE00B080C`). Returns `None` if the block does not hold it.
    pub fn from_resident_block(dec: &[u8]) -> Option<AnimSelector> {
        let cont = find_container(dec, ANIMLOOKUP)?;
        let lookup = AnimTable::parse(&cont)?;
        let h = lookup.col("Handle")?;
        let cn = lookup.col("CharacterName")?;
        let an = lookup.col("Animation")?;
        let lk_ext = (|| {
            Some((
                lookup.col("Gender")?,
                lookup.col("PrimaryEquipmentClass")?,
                lookup.col("PrimaryEquipmentName")?,
                lookup.col("InUseEquipmentClass")?,
                lookup.col("InUseEquipmentName")?,
                lookup.col("MinTimeScale")?,
                lookup.col("MaxTimeScale")?,
            ))
        })();
        let actions = find_container(dec, ACTIONTABLE)
            .and_then(|c| AnimTable::parse(&c))
            .and_then(|t| {
                let cols = ActCols {
                    stance: t.col("Stance")?,
                    action: t.col("Action")?,
                    aim_state: t.col("AimState")?,
                    tandem: t.col("Tandem")?,
                    seat: t.col("Seat")?,
                    target: t.col("Target")?,
                    action_direction: t.col("ActionDirection")?,
                    damage_direction: t.col("DamageDirection")?,
                    animation_handles: t.col("AnimationHandles")?,
                    partition_mask: t.col("PartitionMask")?,
                    looping: t.col("Looping")?,
                    driven: t.col("Driven")?,
                    action_mask: t.col("ActionMask")?,
                    locomotion_mask: t.col("LocomotionMask")?,
                };
                Some((t, cols))
            });
        Some(AnimSelector { lookup, h, cn, an, lk_ext, actions })
    }

    /// `CharacterName` hash for a merc — `pandemic_hash_m2(name)` ("mattias"/"chris"/"jennifer").
    pub fn character_name(merc: &str) -> u32 {
        pandemic_hash_m2(merc)
    }

    /// Resolve a Handle to a clip hash for a character (first matching lookup row →
    /// `ASTO[Animation]`).
    pub fn resolve_handle(&self, handle: u32, character: u32) -> Option<u32> {
        let td = self.lookup.total_dims;
        for row in 0..self.lookup.row_count() {
            let base = row * td;
            if self.lookup.rows[base + self.h] == handle && self.lookup.rows[base + self.cn] == character {
                let idx = self.lookup.rows[base + self.an] as usize;
                return self.lookup.asto.get(idx).copied();
            }
        }
        None
    }

    /// The character's primary standing idle clip.
    pub fn primary_idle(&self, character: u32) -> Option<u32> {
        self.resolve_handle(PRIMARY_IDLE_HANDLE, character)
    }

    /// Every AnimationLookup row keyed to `character`, resolved through `ASTO` — the
    /// character's own animation set, in table order (equipment/gender variant rows share a
    /// Handle and resolve to their own clips). Empty when the character has no personal rows.
    pub fn character_clips(&self, character: u32) -> Vec<CharacterClip> {
        let td = self.lookup.total_dims;
        let mut out = Vec::new();
        for row in 0..self.lookup.row_count() {
            let base = row * td;
            if self.lookup.rows[base + self.cn] == character {
                let idx = self.lookup.rows[base + self.an] as usize;
                if let Some(&clip) = self.lookup.asto.get(idx) {
                    out.push(CharacterClip { handle: self.lookup.rows[base + self.h], clip });
                }
            }
        }
        out
    }

    /// All ActionTable rows whose `AnimationHandles` is `handle` — the game states (Stance/
    /// Action/AimState/ActionDirection/…) that play this handle. Empty if the block carried no
    /// ActionTable.
    pub fn handle_actions(&self, handle: u32) -> Vec<ActionRow> {
        let Some((t, c)) = &self.actions else { return Vec::new() };
        let td = t.total_dims;
        let mut out = Vec::new();
        for row in 0..t.row_count() {
            let base = row * td;
            if t.rows[base + c.animation_handles] != handle {
                continue;
            }
            out.push(ActionRow {
                stance: t.rows[base + c.stance],
                action: t.rows[base + c.action],
                aim_state: t.rows[base + c.aim_state],
                tandem: t.rows[base + c.tandem],
                seat: t.rows[base + c.seat],
                target: t.rows[base + c.target],
                action_direction: t.rows[base + c.action_direction],
                damage_direction: t.rows[base + c.damage_direction],
                partition_mask: t.rows[base + c.partition_mask],
                looping: t.rows[base + c.looping],
                driven: t.rows[base + c.driven],
                action_mask: t.rows[base + c.action_mask],
                locomotion_mask: t.rows[base + c.locomotion_mask],
            });
        }
        out
    }

    /// The AnimationLookup rows for (`handle`, `character`) with their equipment/gender keys and
    /// timescale range — how the SAME Handle resolves to different clips per loadout. Empty when
    /// the retail extended columns are absent.
    pub fn lookup_context(&self, handle: u32, character: u32) -> Vec<LookupContext> {
        let Some((g, pec, pen, iec, ien, mints, maxts)) = self.lk_ext else { return Vec::new() };
        let td = self.lookup.total_dims;
        let mut out = Vec::new();
        for row in 0..self.lookup.row_count() {
            let base = row * td;
            if self.lookup.rows[base + self.h] != handle
                || self.lookup.rows[base + self.cn] != character
            {
                continue;
            }
            let idx = self.lookup.rows[base + self.an] as usize;
            out.push(LookupContext {
                handle,
                gender: self.lookup.rows[base + g],
                primary_equipment_class: self.lookup.rows[base + pec],
                primary_equipment_name: self.lookup.rows[base + pen],
                in_use_equipment_class: self.lookup.rows[base + iec],
                in_use_equipment_name: self.lookup.rows[base + ien],
                clip: self.lookup.asto.get(idx).copied().unwrap_or(0),
                min_time_scale: f32::from_bits(self.lookup.rows[base + mints]),
                max_time_scale: f32::from_bits(self.lookup.rows[base + maxts]),
            });
        }
        out
    }
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

/// One AnimationLookup row's equipment context: which loadout the (Handle, CharacterName)
/// pair resolves to `clip` under. `min/max_time_scale` = allowed playback-rate range
/// (`-1.0` = default).
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

/// One AnimationLookup row resolved for a character: the logical Handle (the ActionTable's
/// `AnimationHandles` join key) and the clip hash `ASTO[Animation]` yields.
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

/// Static fallback (validated) for the primary idle, used only if the resident
/// block can't be parsed. Keeps the correct per-merc idle even on a WAD variant.
pub fn fallback_idle(character: u32) -> Option<u32> {
    match character {
        0x030E_6C38 => Some(0x6EA8_8E00), // mattias
        0xD64B_B122 => Some(0x835D_A06A), // chris
        0xF314_4C8E => Some(0x24F8_C8E6), // jennifer
        _ => None,
    }
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

    /// Build one UCFX animationtable container: `INFO → TYPE → [ASTO] → VALU`.
    fn synth_table(cols: &[&str], rows: &[&[u32]], asto_vals: &[u32]) -> Vec<u8> {
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
        for &v in asto_vals {
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
        if !asto_vals.is_empty() {
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

    /// Minimal resident block: one AnimationLookup with columns
    /// `Handle, CharacterName, Animation` and two rows (mattias, chris) → an ASTO pool.
    fn synth_block() -> Vec<u8> {
        let cont = synth_table(
            &["Handle", "CharacterName", "Animation"],
            &[
                &[PRIMARY_IDLE_HANDLE, 0x030E_6C38, 1], // mattias -> asto[1] = 0xBBBB
                &[PRIMARY_IDLE_HANDLE, 0xD64B_B122, 2], // chris   -> asto[2] = 0xCCCC
            ],
            &[0xAAAA, 0xBBBB, 0xCCCC],
        );
        synth_resident(&[(ANIMLOOKUP, &cont)])
    }

    #[test]
    fn resolves_per_character_idle() {
        let block = synth_block();
        assert!(block_has_lookup(&block));
        let sel = AnimSelector::from_resident_block(&block).expect("selector builds");
        assert_eq!(sel.primary_idle(0x030E_6C38), Some(0xBBBB)); // mattias
        assert_eq!(sel.primary_idle(0xD64B_B122), Some(0xCCCC)); // chris
        assert_eq!(sel.primary_idle(0xF314_4C8E), None); // jennifer not in this synth table
    }

    #[test]
    fn action_table_and_lookup_context() {
        // Full retail column sets: an ActionTable row keyed Upright/Fidget playing Handle
        // PRIMARY_IDLE_HANDLE, and a lookup with the extended equipment/timescale columns.
        let at = synth_table(
            &[
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
            ],
            &[
                &[
                    0x12C0_7B18, // Stance = Upright (named in the devkit strings)
                    0x0C0A_7FA6, // Action = Fidget
                    NONE_SENTINEL,
                    NONE_SENTINEL,
                    NONE_SENTINEL,
                    NONE_SENTINEL,
                    NONE_SENTINEL,
                    NONE_SENTINEL,
                    PRIMARY_IDLE_HANDLE,
                    0,
                    1, // Looping
                    0,
                    0,
                    0,
                ],
                // A second row on a DIFFERENT handle must not match.
                &[
                    0x12C0_7B18,
                    0xDEAD_BEEF,
                    NONE_SENTINEL,
                    NONE_SENTINEL,
                    NONE_SENTINEL,
                    NONE_SENTINEL,
                    NONE_SENTINEL,
                    NONE_SENTINEL,
                    0x1111_1111,
                    0,
                    0,
                    0,
                    0,
                    0,
                ],
            ],
            &[],
        );
        let lk = synth_table(
            &[
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
            ],
            &[&[
                PRIMARY_IDLE_HANDLE,
                NONE_SENTINEL,
                0x030E_6C38, // mattias
                0xCAFE_0001, // PrimaryEquipmentClass
                NONE_SENTINEL,
                NONE_SENTINEL,
                NONE_SENTINEL,
                1,
                (-1.0f32).to_bits(),
                (-1.0f32).to_bits(),
            ]],
            &[0xAAAA, 0xBBBB, 0xCCCC],
        );
        let block = synth_resident(&[(ANIMLOOKUP, &lk), (ACTIONTABLE, &at)]);
        let sel = AnimSelector::from_resident_block(&block).expect("selector builds");

        let acts = sel.handle_actions(PRIMARY_IDLE_HANDLE);
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].stance, 0x12C0_7B18);
        assert_eq!(acts[0].action, 0x0C0A_7FA6);
        assert_eq!(acts[0].aim_state, NONE_SENTINEL);
        assert_eq!(acts[0].looping, 1);
        assert!(sel.handle_actions(0x2222_2222).is_empty());

        let ctx = sel.lookup_context(PRIMARY_IDLE_HANDLE, 0x030E_6C38);
        assert_eq!(ctx.len(), 1);
        assert_eq!(ctx[0].clip, 0xBBBB);
        assert_eq!(ctx[0].primary_equipment_class, 0xCAFE_0001);
        assert_eq!(ctx[0].min_time_scale, -1.0);
        // Wrong character → no rows.
        assert!(sel.lookup_context(PRIMARY_IDLE_HANDLE, 0xF314_4C8E).is_empty());
    }

    #[test]
    fn character_name_and_fallback() {
        // The keys the engine uses are pandemic_hash_m2 of the merc name.
        assert_eq!(AnimSelector::character_name("mattias"), 0x030E_6C38);
        assert_eq!(AnimSelector::character_name("chris"), 0xD64B_B122);
        assert_eq!(AnimSelector::character_name("jennifer"), 0xF314_4C8E);
        // Validated fallbacks (used only if the resident block can't be parsed).
        assert_eq!(fallback_idle(0x030E_6C38), Some(0x6EA8_8E00));
        assert_eq!(fallback_idle(0xF314_4C8E), Some(0x24F8_C8E6));
    }
}
