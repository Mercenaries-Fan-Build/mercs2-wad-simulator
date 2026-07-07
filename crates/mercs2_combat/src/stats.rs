//! `wpn_*` weapon-stat loader.
//!
//! Editable gun stats live in **26 `wpn_*` reflection blocks** in the WAD (NOT Lua — memory
//! [[weapon-definitions-wpn-blocks]], code map §7). Each block is `[u32 count][count×16B entries]
//! [bodies…]`; entry **[0]** (type-hash `0x9f8bca10` = the weapon def) is a UCFX container whose single
//! `data` chunk is a **reflection blob**: a directory followed by an array of sub-objects, each headed
//! by the tag `0x787c0871` (`= pandemic_hash_m2("weapon")`). The sub-objects are the authored
//! `WeaponProjectileBase` / `WeaponScatter` / `ProjectilePhysics` / `HomingWeapon` / … component
//! instances, serialized **positionally** (the field schema is declared by the exe reflection
//! templates `FUN_0065ca70` et al., ecs-01 §schemas — the names are not in the block).
//!
//! ## What this loader does (faithful) vs. does not (confirm-live)
//! - **Does**, reliably (verified against a real block, [`tests::live_parse_real_wpn_block`]): unwrap a
//!   `wpn_*` block → the weapon-def UCFX `data` chunk → enumerate its `0x787c0871` sub-objects with
//!   their raw field words. Endian-aware: retail PC `vz.wad` is little-endian; the Xbox/PS3 source is
//!   big-endian (magic stored `XFCU`, tag `atad`).
//! - **Does not** (see `DEFERRED.md`): map each sub-object word to a *named* stat. The byte offset →
//!   `RateOfFire`/`iClipSize` binding is positional and unpinned on disk; forcing it would invent
//!   numbers (the RE note reached the same conclusion). So [`WeaponStats`] carries the **documented exe
//!   schema defaults** (real values from ecs-01, not invented) and the parsed sub-objects are exposed
//!   raw for the confirm-live follow-up.

/// ASET type-hash of a `wpn_*` block's entry[0] — the weapon-definition reflection container.
pub const WEAPON_DEF_TYPE_HASH: u32 = 0x9f8b_ca10;
/// The sub-object tag inside the weapon-def blob (`pandemic_hash_m2("weapon")`).
pub const WEAPON_SUBOBJECT_TAG: u32 = 0x787c_0871;

/// `WeaponProjectileTypeEnum::FireType` — how the trigger drives the barrel (ecs-01 WeaponProjectileBase
/// field 1). The default authored value is `Automatic`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FireType {
    /// Fires continuously while the trigger is held (default).
    #[default]
    Automatic,
    /// One shot per trigger pull.
    SemiAutomatic,
    /// A fixed burst per pull.
    Burst,
}

/// Core gun statistics for one weapon, mirroring the authored `WeaponProjectileBase` / `WeaponScatter`
/// / `ProjectilePhysics` / `HomingWeapon` / `Explosive` reflection classes (ecs-01 §schemas). Values
/// default to the **exe schema defaults** (the real recovered defaults — `iClipSize 30`, `RateOfFire
/// 120`, …); per-weapon overrides come from the `wpn_*` blob once the offset→field binding is pinned
/// (confirm-live, `DEFERRED.md`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponStats {
    // --- WeaponProjectileBase (0xeb505c8b, stride 0x28) — core gun stats ---
    /// FireType (field 1). Default `Automatic`.
    pub fire_type: FireType,
    /// `iClipSize` (field 6). Rounds per magazine. Default **30**.
    pub clip_size: i32,
    /// `MaxAmmoReserve` (field 7). Carried reserve rounds. Default **60**.
    pub max_ammo_reserve: i32,
    /// `iBulletsPerShot` (field 9). >1 = shotgun-style pellets. Default **1**.
    pub bullets_per_shot: i32,
    /// `iRoundsPerReload` (field 10). `-1` = reload the whole clip at once. Default **-1**.
    pub rounds_per_reload: i32,
    /// `RateOfFire` (field 11), **rounds per minute**. Default **120.0**. Fire interval = `60/rof` s.
    pub rate_of_fire: f32,
    /// `MaxAimAngleAi` (field 17), degrees. Default **15.0**.
    pub max_aim_angle_ai: f32,

    // --- WeaponScatter (0xe7234615, stride 0x1c) — spread ---
    /// `ScatterMin` (field 5), degrees of cone half-angle at best accuracy. Default 1.5.
    pub scatter_min: f32,
    /// `ScatterMax` (field 6), degrees at worst accuracy. Default 1.5.
    pub scatter_max: f32,

    // --- ProjectilePhysics (0x11e6c283, stride 0x28) — ballistics ---
    /// Muzzle velocity (m/s). `0` ⇒ hitscan (instant raycast); `>0` ⇒ a spawned projectile. The
    /// ProjectilePhysics `Velocity`-class default is 10.0; guns default to hitscan (0) unless the blob
    /// authors a projectile.
    pub muzzle_velocity: f32,
    /// Gravity acceleration applied to a spawned projectile (m/s²), +down. `0` for a flat tracer.
    pub projectile_gravity: f32,
    /// Projectile lifetime (s) before it self-detonates / despawns. Default 6.0.
    pub projectile_lifetime: f32,

    // --- damage payload (fed to the damage applier) ---
    /// Base damage a single hit deals at point-blank (before falloff). Default 10.0 (the
    /// ProjectilePhysics velocity-class-adjacent default; per-weapon in the blob).
    pub damage: f32,
    /// The damage taxonomy key this weapon deals (drives the destruction reaction, code map §5.1).
    pub damage_key: crate::damage::DamageKey,

    // --- Explosive (0xf74044ba, stride 0x24) — for explosive rounds/warheads ---
    /// If `Some`, a hit spawns a `RuntimeExplosion` with these params instead of a point hit.
    pub explosive: Option<ExplosiveStats>,

    // --- HomingWeapon (0x1a4db6ed, stride 0x18) — if this is a lock-on launcher ---
    /// If `Some`, this weapon is a homing/lock-on launcher (Stinger-class).
    pub homing: Option<HomingStats>,

    /// `IsDesignator` (Lua `Weapon.IsDesignator`, code map §7) — a laser designator that paints targets
    /// for airstrikes rather than dealing direct damage. Default `false`.
    pub designator: bool,
}

/// `Explosive` reflection fields (ecs-01) — a blast's radius/force/damage/falloff.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExplosiveStats {
    /// Blast radius (m). Beyond this, zero damage/force.
    pub radius: f32,
    /// `MaxForce` (field 3) — peak impulse at the centre. Default 1.0 (scaled by the authored blob).
    pub max_force: f32,
    /// `Damage`-group (field 5) — peak damage at the centre. Default 0.3 (blob overrides).
    pub damage: f32,
    /// `MinForceFalloff` — falloff shape control (0 = linear to the edge).
    pub min_force_falloff: f32,
}

impl Default for ExplosiveStats {
    fn default() -> Self {
        // Explosive schema defaults (ecs-01): MaxForce 1.0, Damage-group 0.3, Arc 20.0.
        Self {
            radius: 20.0,
            max_force: 1.0,
            damage: 0.3,
            min_force_falloff: 0.0,
        }
    }
}

/// `HomingWeapon` reflection fields (ecs-01 HomingWeapon schema `FUN_0065d930`, stride 0x18) — the
/// authored inputs to the lock FSM (`FUN_0052dce0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HomingStats {
    /// `LockOnMinWeight` (field 2). Minimum target weight to hold a lock. Default 0.0.
    pub lock_on_min_weight: f32,
    /// `LockOnMaxAngle` (field 3), degrees off the aim axis a target can be and still lock. Default 10.
    pub lock_on_max_angle: f32,
    /// `LockOnMaxDistance` (field 4), m. Default 100.
    pub lock_on_max_distance: f32,
    /// `LockOnTime` (field 5), s the reticle must hold the target to acquire. Default 1.0.
    pub lock_on_time: f32,
    // --- HomingProjectile (0xe81b2874, stride 0x0c) — guided flight (defaults 10.0, 0.3, 0.3) ---
    /// `TurnSpeed` — the guided-flight steering rate (`FUN_0052e1f0` cross-product term). Default 10.0.
    pub turn_speed: f32,
    /// Detonation proximity distance to the target (HomingTarget field, default ~0.2 → widened to a
    /// usable proximity). The missile detonates within this of the locked target, or on the arm timer.
    pub detonation_distance: f32,
}

impl Default for HomingStats {
    fn default() -> Self {
        Self {
            lock_on_min_weight: 0.0,
            lock_on_max_angle: 10.0,
            lock_on_max_distance: 100.0,
            lock_on_time: 1.0,
            turn_speed: 10.0,
            detonation_distance: 2.0,
        }
    }
}

impl Default for WeaponStats {
    /// The **exe schema defaults** (ecs-01 §WeaponProjectileBase / WeaponScatter / ProjectilePhysics).
    /// These are recovered defaults, not invented placeholders — a weapon that does not override a
    /// field in its `wpn_*` blob genuinely uses these.
    fn default() -> Self {
        Self {
            fire_type: FireType::Automatic,
            clip_size: 30,
            max_ammo_reserve: 60,
            bullets_per_shot: 1,
            rounds_per_reload: -1,
            rate_of_fire: 120.0,
            max_aim_angle_ai: 15.0,
            scatter_min: 1.5,
            scatter_max: 1.5,
            muzzle_velocity: 0.0, // hitscan by default
            projectile_gravity: 0.0,
            projectile_lifetime: 6.0,
            damage: 10.0,
            damage_key: crate::damage::DamageKey::BulletLarge,
            explosive: None,
            homing: None,
            designator: false,
        }
    }
}

impl WeaponStats {
    /// Seconds between shots from `rate_of_fire` (rounds/minute). Guards a zero/negative RoF to one
    /// shot/second so a mis-authored block can't divide-by-zero the fire loop.
    pub fn fire_interval(&self) -> f32 {
        if self.rate_of_fire > 0.0 {
            60.0 / self.rate_of_fire
        } else {
            1.0
        }
    }

    /// A rocket-launcher preset (homing Stinger-class): slow projectile, explosive warhead, lock-on.
    /// Used where a homing weapon is needed before the `wpn_rocket` blob's overrides are pinned.
    pub fn rocket_launcher() -> Self {
        Self {
            fire_type: FireType::SemiAutomatic,
            clip_size: 1,
            max_ammo_reserve: 8,
            rate_of_fire: 40.0,
            muzzle_velocity: 45.0,
            projectile_gravity: 3.0,
            projectile_lifetime: 8.0,
            damage: 120.0,
            damage_key: crate::damage::DamageKey::RocketLarge,
            explosive: Some(ExplosiveStats {
                radius: 8.0,
                max_force: 20.0,
                damage: 120.0,
                min_force_falloff: 0.0,
            }),
            homing: Some(HomingStats::default()),
            ..Self::default()
        }
    }
}

// ---------------------------------------------------------------------------
// wpn_* block parsing
// ---------------------------------------------------------------------------

/// One `0x787c0871`-tagged sub-object inside the weapon-def reflection blob: its byte offset within the
/// `data` chunk and the raw field words following the tag (endian-normalised to host `u32`s). The
/// offset→named-stat binding is confirm-live (`DEFERRED.md`); this is the faithful raw surface.
#[derive(Clone, Debug, PartialEq)]
pub struct WeaponSubObject {
    /// Byte offset of the tag within the `data` chunk.
    pub offset: usize,
    /// The field words after the tag, up to the next sub-object (or blob end).
    pub words: Vec<u32>,
}

impl WeaponSubObject {
    /// Interpret field word `i` as an `f32` (the exe stores stats as plain IEEE floats).
    pub fn f32(&self, i: usize) -> Option<f32> {
        self.words.get(i).map(|&w| f32::from_bits(w))
    }
    /// Interpret field word `i` as an `i32`.
    pub fn i32(&self, i: usize) -> Option<i32> {
        self.words.get(i).map(|&w| w as i32)
    }
}

/// A parsed `wpn_*` weapon-definition blob: the enumerated `0x787c0871` sub-objects and the raw `data`
/// chunk they came from. See the module docs for what is / isn't recovered.
#[derive(Clone, Debug)]
pub struct WeaponDefBlob {
    /// The weapon-def `data` reflection chunk (endian as stored on disk).
    pub data: Vec<u8>,
    /// Endianness of `data` (`true` = big-endian Xbox/PS3 source; `false` = retail PC LE).
    pub big_endian: bool,
    /// The `0x787c0871` sub-objects in blob order.
    pub sub_objects: Vec<WeaponSubObject>,
}

fn rd_u32(b: &[u8], o: usize, be: bool) -> Option<u32> {
    let s = b.get(o..o + 4)?;
    let a = [s[0], s[1], s[2], s[3]];
    Some(if be { u32::from_be_bytes(a) } else { u32::from_le_bytes(a) })
}

/// Locate a `wpn_*` block's weapon-definition container (entry with type-hash [`WEAPON_DEF_TYPE_HASH`])
/// inside a raw block `[u32 count][count×16B entries][bodies…]` and return its container bytes.
///
/// `big_endian` selects how the block table's `u32`s are read. Returns `None` if the block is
/// malformed or has no weapon-def entry.
pub fn find_weapon_def_container(block: &[u8], big_endian: bool) -> Option<&[u8]> {
    let count = rd_u32(block, 0, big_endian)? as usize;
    // Guard: the entry table must fit.
    let table_end = 4usize.checked_add(count.checked_mul(16)?)?;
    if count == 0 || count > 64 || table_end > block.len() {
        return None;
    }
    // Bodies follow the table, in entry order.
    let mut pos = table_end;
    for i in 0..count {
        let base = 4 + i * 16;
        let type_hash = rd_u32(block, base + 4, big_endian)?;
        let chunk_size = rd_u32(block, base + 12, big_endian)? as usize;
        let end = pos.checked_add(chunk_size)?;
        if end > block.len() {
            return None;
        }
        if type_hash == WEAPON_DEF_TYPE_HASH {
            return Some(&block[pos..end]);
        }
        pos = end;
    }
    None
}

/// Extract the single `data` reflection chunk from a weapon-def UCFX container. Endian-aware: the magic
/// is `UCFX` (LE) or `XFCU` (BE source), the descriptor tag `data` (LE) or `atad` (BE source). Returns
/// the chunk bytes (still in source endianness).
fn extract_data_chunk(container: &[u8], big_endian: bool) -> Option<Vec<u8>> {
    if container.len() < 20 {
        return None;
    }
    let magic = &container[0..4];
    let ucfx = if big_endian { b"XFCU" } else { b"UCFX" };
    if magic != ucfx {
        return None;
    }
    let data_area_off = rd_u32(container, 4, big_endian)? as usize;
    let n_desc = rd_u32(container, 16, big_endian)? as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc == 0 || n_desc > max_desc {
        return None;
    }
    let data_start = if data_area_off > 0 { data_area_off } else { 20 + n_desc * 20 };
    let want_tag: &[u8; 4] = if big_endian { b"atad" } else { b"data" };
    for i in 0..n_desc {
        let ro = 20 + i * 20;
        let tag = &container[ro..ro + 4];
        if tag == want_tag {
            let u0 = rd_u32(container, ro + 4, big_endian)? as usize;
            let sz = rd_u32(container, ro + 8, big_endian)? as usize;
            let s = data_start.checked_add(u0)?;
            let e = s.checked_add(sz)?;
            if e <= container.len() {
                return Some(container[s..e].to_vec());
            }
        }
    }
    None
}

/// Scan a weapon-def `data` chunk for `0x787c0871` sub-object tags, returning each with its trailing
/// field words. The 4-byte-aligned scan is robust to the (unpinned) directory header at the top of the
/// blob.
fn scan_sub_objects(data: &[u8], big_endian: bool) -> Vec<WeaponSubObject> {
    let mut tag_offsets = Vec::new();
    let mut o = 0;
    while o + 4 <= data.len() {
        if rd_u32(data, o, big_endian) == Some(WEAPON_SUBOBJECT_TAG) {
            tag_offsets.push(o);
        }
        o += 4;
    }
    let mut subs = Vec::with_capacity(tag_offsets.len());
    for (k, &off) in tag_offsets.iter().enumerate() {
        let end = tag_offsets.get(k + 1).copied().unwrap_or(data.len());
        // Field words start right after the 4-byte tag.
        let mut words = Vec::new();
        let mut w = off + 4;
        while w + 4 <= end {
            if let Some(v) = rd_u32(data, w, big_endian) {
                words.push(v);
            }
            w += 4;
        }
        subs.push(WeaponSubObject { offset: off, words });
    }
    subs
}

/// Parse a raw `wpn_*` block into its weapon-definition blob. `big_endian` = the source endianness
/// (retail PC `vz.wad` is `false`; the Xbox/PS3 source is `true`). Returns `None` if the block has no
/// weapon-def entry or the container is malformed.
pub fn parse_weapon_block(block: &[u8], big_endian: bool) -> Option<WeaponDefBlob> {
    let container = find_weapon_def_container(block, big_endian)?;
    let data = extract_data_chunk(container, big_endian)?;
    let sub_objects = scan_sub_objects(&data, big_endian);
    Some(WeaponDefBlob {
        data,
        big_endian,
        sub_objects,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fire_interval_from_rof() {
        let s = WeaponStats::default();
        assert!((s.fire_interval() - 0.5).abs() < 1e-6); // 120 rpm → 0.5 s
        let mut z = WeaponStats::default();
        z.rate_of_fire = 0.0;
        assert_eq!(z.fire_interval(), 1.0); // guard, no div-by-zero
    }

    #[test]
    fn defaults_are_the_exe_schema_defaults() {
        let s = WeaponStats::default();
        assert_eq!(s.clip_size, 30);
        assert_eq!(s.max_ammo_reserve, 60);
        assert_eq!(s.bullets_per_shot, 1);
        assert_eq!(s.rate_of_fire, 120.0);
        assert_eq!(s.max_aim_angle_ai, 15.0);
    }

    /// Synthetic weapon block round-trip (LE): build `[count][entries][UCFX data w/ 2 weapon
    /// sub-objects]`, parse it, and assert the sub-objects + their float fields come back.
    #[test]
    fn parse_synthetic_le_block() {
        // --- build the weapon-def data chunk: [dir word][tag][f=0.96][f=10][tag][f=1.5] ---
        let mut data = Vec::new();
        data.extend_from_slice(&2u32.to_le_bytes()); // a directory word (ignored by the scan)
        data.extend_from_slice(&WEAPON_SUBOBJECT_TAG.to_le_bytes());
        data.extend_from_slice(&0.96f32.to_bits().to_le_bytes());
        data.extend_from_slice(&10.0f32.to_bits().to_le_bytes());
        data.extend_from_slice(&WEAPON_SUBOBJECT_TAG.to_le_bytes());
        data.extend_from_slice(&1.5f32.to_bits().to_le_bytes());

        // --- wrap in a UCFX container with one `data` descriptor ---
        let mut container = Vec::new();
        container.extend_from_slice(b"UCFX");
        let data_area_off = 20 + 20; // header + 1 desc row
        container.extend_from_slice(&(data_area_off as u32).to_le_bytes());
        container.extend_from_slice(&0u32.to_le_bytes());
        container.extend_from_slice(&0u32.to_le_bytes());
        container.extend_from_slice(&1u32.to_le_bytes()); // n_desc
        container.extend_from_slice(b"data");
        container.extend_from_slice(&0u32.to_le_bytes()); // u0
        container.extend_from_slice(&(data.len() as u32).to_le_bytes()); // sz
        container.extend_from_slice(&[0u8; 8]); // row pad
        container.extend_from_slice(&data);

        // --- wrap the container in a block with 1 entry (the weapon def) ---
        let mut block = Vec::new();
        block.extend_from_slice(&1u32.to_le_bytes()); // count
        block.extend_from_slice(&0x1234_5678u32.to_le_bytes()); // name_hash
        block.extend_from_slice(&WEAPON_DEF_TYPE_HASH.to_le_bytes()); // type_hash
        block.extend_from_slice(&0u32.to_le_bytes()); // field_c
        block.extend_from_slice(&(container.len() as u32).to_le_bytes()); // chunk_size
        block.extend_from_slice(&container);

        let blob = parse_weapon_block(&block, false).expect("parse");
        assert_eq!(blob.sub_objects.len(), 2);
        assert!((blob.sub_objects[0].f32(0).unwrap() - 0.96).abs() < 1e-6);
        assert!((blob.sub_objects[0].f32(1).unwrap() - 10.0).abs() < 1e-6);
        assert!((blob.sub_objects[1].f32(0).unwrap() - 1.5).abs() < 1e-6);
    }

    /// Live parse of a **real** `wpn_*` block (the big-endian sniper-rifle source dump). SKIPS (passes)
    /// when the fixture is absent so CI stays green, matching the crate's other live tests.
    /// Asserts the verified structure: 3 ASET entries, a weapon-def container, and 8 weapon
    /// sub-objects (empirically confirmed for `wpn_sniperrifle`).
    #[test]
    fn live_parse_real_wpn_block() {
        // The dump is the raw BE Xbox/PS3 block. Env override for other environments; otherwise built
        // from the crate dir so it doesn't depend on the test's cwd.
        let path = std::env::var("WPN_BLOCK").unwrap_or_else(|_| {
            format!(
                "{}/../../../../notes-on-the-released-game/output/temp_blocks/\
                 02986_blocks_vz_wpn_sniperrifle_P000_Q3.block.bin",
                env!("CARGO_MANIFEST_DIR")
            )
        });
        let Ok(block) = std::fs::read(&path) else {
            eprintln!("skip: wpn block fixture not present at {path}");
            return;
        };
        // Real block is big-endian source.
        let blob = parse_weapon_block(&block, true).expect("parse real wpn block");
        assert!(
            !blob.sub_objects.is_empty(),
            "real weapon-def blob has 0x787c0871 sub-objects"
        );
        // wpn_sniperrifle: 8 weapon sub-objects (verified during RE).
        assert_eq!(blob.sub_objects.len(), 8, "sniperrifle sub-object count");
        // Every sub-object's fields decode to finite floats (sanity, not a stat claim).
        for s in &blob.sub_objects {
            assert!(!s.words.is_empty());
            assert!(s.f32(2).map_or(true, |v| v.is_finite()));
        }
    }
}
