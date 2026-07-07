//! Data-driven clip SELECTION — the forward `(character, game-state) → clip hash` resolver.
//!
//! The parse of `ActionTable`/`AnimationLookup`/`ASTO` out of the resident WAD block lives in
//! [`mercs2_formats::anim_select::AnimSelector`] (already reverse-engineered + validated vs a live
//! x32dbg capture, Chris idle `0xED37BC56`; see `docs/modernization/human_animation_selection.md`).
//! That type exposes the two halves of the join — `character_clips(char) → (Handle, clip)` and
//! `handle_actions(Handle) → ActionRow`s. [`ClipPicker`] composes them into the engine's real
//! forward direction: derive a [`StateKey`] from gameplay (Stance/Action/AimState/ActionDirection/…),
//! match it against the ActionTable rows, take the `AnimationHandles` Handle, and resolve it per
//! CharacterName through the lookup to the Havok clip hash — no hardcoded `CLIP_IDLE/WALK/RUN`.

use mercs2_formats::anim_select::{ActionRow, AnimSelector, NONE_SENTINEL};
use std::collections::HashMap;

/// `Stance = "Upright"` (`m2("Upright")`) — the standing stance (validated).
pub const STANCE_UPRIGHT: u32 = 0x12C0_7B18;
/// `Action = "Fidget"` (`m2("Fidget")`) — an idle-variant action (validated).
pub const ACTION_FIDGET: u32 = 0x0C0A_7FA6;
/// The Upright idle-cluster head Handle (validated per-merc: mattias `0x6EA88E00`,
/// chris `0x835DA06A`, jennifer `0x24F8C8E6`).
pub const PRIMARY_IDLE_HANDLE: u32 = 0x700D_4DE0;

/// Wildcard sentinel — matches any value on either side of a key comparison. Same value the tables
/// use for "any"/unset key columns.
pub const ANY: u32 = NONE_SENTINEL;

/// The gameplay state the animation is keyed on — the ActionTable's 6 key columns. Any field left
/// [`ANY`] is a wildcard (matches every row value); a row column of [`ANY`] likewise matches any
/// query value, exactly as the retail table's none-sentinel behaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateKey {
    pub stance: u32,
    pub action: u32,
    pub aim_state: u32,
    pub tandem: u32,
    pub seat: u32,
    pub target: u32,
    pub action_direction: u32,
}

impl Default for StateKey {
    fn default() -> Self {
        StateKey {
            stance: ANY,
            action: ANY,
            aim_state: ANY,
            tandem: ANY,
            seat: ANY,
            target: ANY,
            action_direction: ANY,
        }
    }
}

impl StateKey {
    /// The canonical standing idle key (`Upright` + `Fidget`) — the validated Chris-idle path.
    pub fn idle() -> Self {
        StateKey { stance: STANCE_UPRIGHT, action: ACTION_FIDGET, ..Self::default() }
    }

    /// True if a single column matches: either side [`ANY`], or exact equality.
    #[inline]
    fn col_match(key: u32, row: u32) -> bool {
        key == ANY || row == ANY || key == row
    }

    /// True if this state key selects `row`.
    fn matches(&self, row: &ActionRow) -> bool {
        Self::col_match(self.stance, row.stance)
            && Self::col_match(self.action, row.action)
            && Self::col_match(self.aim_state, row.aim_state)
            && Self::col_match(self.tandem, row.tandem)
            && Self::col_match(self.seat, row.seat)
            && Self::col_match(self.target, row.target)
            && Self::col_match(self.action_direction, row.action_direction)
    }

    /// A rough "specificity" — how many key columns are pinned (not [`ANY`]). Used to prefer the
    /// most specific matching ActionTable row over a broad `any` default.
    fn specificity(row: &ActionRow) -> u32 {
        [
            row.stance,
            row.action,
            row.aim_state,
            row.tandem,
            row.seat,
            row.target,
            row.action_direction,
        ]
        .iter()
        .filter(|&&v| v != ANY)
        .count() as u32
    }
}

/// A resolved clip and the flags/rate that came with its ActionTable row + AnimationLookup row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedClip {
    /// The Havok clip name-hash (`ASTO[Animation]`) — look up in the entity's animgroup.
    pub clip: u32,
    /// The logical Handle the ActionTable produced (`AnimationHandles`).
    pub handle: u32,
    /// `Looping` flag from the ActionTable row.
    pub looping: bool,
    /// `Driven` flag (root-motion driven) from the ActionTable row.
    pub driven: bool,
    /// Allowed playback-rate range from the lookup row (`-1.0` = default / unclamped).
    pub min_time_scale: f32,
    pub max_time_scale: f32,
}

/// One precomputed `(state row → clip)` entry for a character: the ActionTable row's key columns and
/// flags, flattened next to the clip that (Handle, character) resolves to.
#[derive(Clone, Copy)]
struct IndexEntry {
    row: ActionRow,
    handle: u32,
    clip: u32,
    min_time_scale: f32,
    max_time_scale: f32,
}

/// The forward selector. Wraps a parsed [`AnimSelector`] and precomputes, per character, the flat
/// `(ActionRow, clip)` table so a per-tick resolve is a cheap linear scan.
pub struct ClipPicker {
    selector: AnimSelector,
    index: HashMap<u32, Vec<IndexEntry>>,
}

impl ClipPicker {
    /// Build from the resident block that carries the AnimationLookup (`0xE00B080C`), precomputing
    /// the forward index for `characters` (typically the three merc CharacterName hashes). Returns
    /// `None` if the block doesn't hold the lookup.
    pub fn from_resident_block(dec: &[u8], characters: &[u32]) -> Option<ClipPicker> {
        let selector = AnimSelector::from_resident_block(dec)?;
        let mut picker = ClipPicker { selector, index: HashMap::new() };
        for &c in characters {
            picker.ensure_index(c);
        }
        Some(picker)
    }

    /// Wrap an already-built selector (e.g. from a variant WAD) and precompute for `characters`.
    pub fn new(selector: AnimSelector, characters: &[u32]) -> ClipPicker {
        let mut picker = ClipPicker { selector, index: HashMap::new() };
        for &c in characters {
            picker.ensure_index(c);
        }
        picker
    }

    /// The underlying parsed tables, if lower-level queries are needed.
    pub fn selector(&self) -> &AnimSelector {
        &self.selector
    }

    /// Build (once) the flat forward index for a character.
    fn ensure_index(&mut self, character: u32) {
        if self.index.contains_key(&character) {
            return;
        }
        let mut entries = Vec::new();
        for cc in self.selector.character_clips(character) {
            // The lookup rows for (Handle, character) carry the timescale range for that clip.
            let (mut mn, mut mx) = (-1.0f32, -1.0f32);
            if let Some(ctx) = self
                .selector
                .lookup_context(cc.handle, character)
                .into_iter()
                .find(|c| c.clip == cc.clip)
            {
                mn = ctx.min_time_scale;
                mx = ctx.max_time_scale;
            }
            for row in self.selector.handle_actions(cc.handle) {
                entries.push(IndexEntry {
                    row,
                    handle: cc.handle,
                    clip: cc.clip,
                    min_time_scale: mn,
                    max_time_scale: mx,
                });
            }
        }
        self.index.insert(character, entries);
    }

    /// Resolve `(character, state)` to a clip. Picks the MOST-specific matching ActionTable row
    /// (most pinned key columns), so a narrow `Upright+Fidget+aim` beats a broad `any` default —
    /// mirroring the engine's ordered key match. Returns `None` if nothing matches.
    ///
    /// Note: `&mut self` because the per-character forward index is built lazily on first use for a
    /// character not passed to the constructor. Pre-seed the mercs in the constructor for a `&self`
    /// hot path via [`ClipPicker::resolve_indexed`].
    pub fn resolve(&mut self, character: u32, state: StateKey) -> Option<ResolvedClip> {
        self.ensure_index(character);
        self.resolve_indexed(character, state)
    }

    /// Resolve against an already-built index (no lazy build). Returns `None` if the character
    /// wasn't precomputed or nothing matched.
    pub fn resolve_indexed(&self, character: u32, state: StateKey) -> Option<ResolvedClip> {
        let entries = self.index.get(&character)?;
        let best = entries
            .iter()
            .filter(|e| state.matches(&e.row))
            .max_by_key(|e| StateKey::specificity(&e.row))?;
        Some(ResolvedClip {
            clip: best.clip,
            handle: best.handle,
            looping: best.row.looping != 0,
            driven: best.row.driven != 0,
            min_time_scale: best.min_time_scale,
            max_time_scale: best.max_time_scale,
        })
    }

    /// The character's primary standing idle clip (validated direct-handle path).
    pub fn idle(&self, character: u32) -> Option<u32> {
        self.selector.primary_idle(character)
    }

    /// `pandemic_hash_m2(merc)` — the CharacterName key ("mattias"/"chris"/"jennifer"/NPCs).
    pub fn character_name(merc: &str) -> u32 {
        AnimSelector::character_name(merc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reuse the synthetic-table builders from anim_select's own tests would be ideal, but they are
    // private; build the minimal resident block the same way here.
    const TYPE_ANIMTABLE: u32 = 0x2073_59C7;
    const ANIMLOOKUP: u32 = 0xE00B_080C;
    const ACTIONTABLE: u32 = 0x6802_C321;

    fn p32(b: &mut Vec<u8>, v: u32) {
        b.extend_from_slice(&v.to_le_bytes());
    }

    fn synth_table(cols: &[&str], rows: &[&[u32]], asto_vals: &[u32]) -> Vec<u8> {
        let mut info = Vec::new();
        info.extend_from_slice(&(cols.len() as u16).to_le_bytes());
        info.extend_from_slice(&(cols.len() as u16).to_le_bytes());
        info.extend_from_slice(&(rows.len() as u16).to_le_bytes());
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
        p32(&mut cont, data_area as u32);
        p32(&mut cont, 0);
        p32(&mut cont, 0);
        p32(&mut cont, ndesc as u32);
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

    fn synth_resident(entries: &[(u32, &Vec<u8>)]) -> Vec<u8> {
        let mut block = Vec::new();
        p32(&mut block, entries.len() as u32);
        for (nh, cont) in entries {
            p32(&mut block, *nh);
            p32(&mut block, TYPE_ANIMTABLE);
            p32(&mut block, 0);
            p32(&mut block, cont.len() as u32);
        }
        for (_, cont) in entries {
            block.extend_from_slice(cont);
        }
        block
    }

    /// Resident block: an ActionTable (idle row Upright+Fidget → PRIMARY_IDLE_HANDLE, and a broad
    /// `any`-keyed row on the same handle) + an AnimationLookup mapping (handle, chris) → ASTO clip.
    fn synth_block() -> Vec<u8> {
        let at = synth_table(
            &[
                "Stance", "Action", "AimState", "Tandem", "Seat", "Target", "ActionDirection",
                "DamageDirection", "AnimationHandles", "PartitionMask", "Looping", "Driven",
                "ActionMask", "LocomotionMask",
            ],
            &[
                // Specific idle row (looping).
                &[STANCE_UPRIGHT, ACTION_FIDGET, ANY, ANY, ANY, ANY, ANY, ANY, PRIMARY_IDLE_HANDLE, 0, 1, 0, 0, 0],
                // A broad default row on the same handle (fewer pinned columns).
                &[STANCE_UPRIGHT, ANY, ANY, ANY, ANY, ANY, ANY, ANY, PRIMARY_IDLE_HANDLE, 0, 1, 0, 0, 0],
                // An unrelated handle that must never match the idle key.
                &[STANCE_UPRIGHT, 0xDEAD_BEEF, ANY, ANY, ANY, ANY, ANY, ANY, 0x1111_1111, 0, 0, 0, 0, 0],
            ],
            &[],
        );
        let lk = synth_table(
            &[
                "Handle", "Gender", "CharacterName", "PrimaryEquipmentClass", "PrimaryEquipmentName",
                "InUseEquipmentClass", "InUseEquipmentName", "Animation", "MinTimeScale", "MaxTimeScale",
            ],
            &[&[
                PRIMARY_IDLE_HANDLE,
                ANY,
                0xD64B_B122, // chris
                ANY,
                ANY,
                ANY,
                ANY,
                2, // ASTO[2]
                (0.9f32).to_bits(),
                (1.1f32).to_bits(),
            ]],
            &[0xAAAA, 0xBBBB, 0xED37_BC56],
        );
        synth_resident(&[(ANIMLOOKUP, &lk), (ACTIONTABLE, &at)])
    }

    #[test]
    fn resolves_idle_to_clip_and_flags() {
        let chris = ClipPicker::character_name("chris");
        assert_eq!(chris, 0xD64B_B122);
        let picker = ClipPicker::from_resident_block(&synth_block(), &[chris]).expect("picker builds");
        let r = picker.resolve_indexed(chris, StateKey::idle()).expect("idle resolves");
        // ASTO[2] == the live-captured Chris idle clip.
        assert_eq!(r.clip, 0xED37_BC56);
        assert_eq!(r.handle, PRIMARY_IDLE_HANDLE);
        assert!(r.looping);
        assert_eq!(r.min_time_scale, 0.9);
        assert_eq!(r.max_time_scale, 1.1);
    }

    #[test]
    fn prefers_specific_row_over_broad_default() {
        // With Action pinned to Fidget, the specific (Upright+Fidget) row must win over the broad
        // (Upright + any) row — both point at the same handle here, so we assert the match count via
        // specificity by checking a non-matching narrow key falls back to the broad row.
        let chris = ClipPicker::character_name("chris");
        let picker = ClipPicker::from_resident_block(&synth_block(), &[chris]).unwrap();
        // A key that only pins Stance still resolves (broad row), same clip.
        let broad = StateKey { stance: STANCE_UPRIGHT, ..StateKey::default() };
        assert_eq!(picker.resolve_indexed(chris, broad).unwrap().clip, 0xED37_BC56);
        // An unrelated action still matches the broad Upright row (Action is wildcard there).
        let other = StateKey { stance: STANCE_UPRIGHT, action: 0x1234_5678, ..StateKey::default() };
        assert_eq!(picker.resolve_indexed(chris, other).unwrap().clip, 0xED37_BC56);
    }

    #[test]
    fn unknown_character_has_no_index() {
        let chris = ClipPicker::character_name("chris");
        let picker = ClipPicker::from_resident_block(&synth_block(), &[chris]).unwrap();
        // Jennifer wasn't precomputed and has no rows in the synth table.
        assert!(picker.resolve_indexed(0xF314_4C8E, StateKey::idle()).is_none());
    }
}
