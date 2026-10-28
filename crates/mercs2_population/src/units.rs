//! What a fired spawner spawns — `FUN_004E4150`'s unit choice over the shipped `PopulationList` data.
//!
//! A spawner carries its owner (the object it is attached to — retail instance `+0x24`), its
//! `UnitList` and its `UnitListWeighting`. [`UnitCatalog::pick`] applies the weighting's rule
//! (`mercs2_formats::unit_list`, which documents each rule and its address) to the list, keeping the
//! `Exhaustive*` bags retail keeps in its `0x00DF9710` container: one per owner for `Exhaustive` /
//! `ExhaustiveOnce`, one per list for `ExhaustiveGlobal`, each filled from the list's entries in data
//! order the first time it is drawn from.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use mercs2_formats::unit_list::{weighting, RuntimeUnitList, UnitBag, UnitListEntry};

/// A spawner's unit choice: whose spawner it is, which list, which rule.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnitChoice {
    /// The owning object (retail instance `+0x24`): the `Exhaustive` bag key and the object whose
    /// health the `Health*` rules read. A spawner with owner 0 never spawns (`FUN_004E4150` returns
    /// before the switch).
    pub owner: u32,
    /// The `PopulationList` key (`UnitList`).
    pub unit_list: u32,
    /// `UnitListWeighting` (`SpawnerWeightTypeEnum`, `mercs2_formats::unit_list::weighting`).
    pub weighting: u32,
}

/// The unit lists, their runtime (sorted) form, the vehicle units, and the live bags.
#[derive(Default, Debug)]
pub struct UnitCatalog {
    entries: BTreeMap<u32, Vec<UnitListEntry>>,
    lists: BTreeMap<u32, RuntimeUnitList>,
    vehicles: BTreeSet<u32>,
    bags: HashMap<u32, UnitBag>,
}

impl UnitCatalog {
    /// Build from every list's entries (data order) and the set of units that are vehicles (the
    /// density gate budgets vehicles and people separately). An error when a list is past
    /// `MAX_LIST_ENTRIES`.
    pub fn new(entries: BTreeMap<u32, Vec<UnitListEntry>>, vehicles: BTreeSet<u32>) -> Result<UnitCatalog, String> {
        let mut lists = BTreeMap::new();
        for (&k, e) in &entries {
            lists.insert(k, RuntimeUnitList::build(e).map_err(|err| format!("PopulationList 0x{k:08X}: {err}"))?);
        }
        Ok(UnitCatalog { entries, lists, vehicles, bags: HashMap::new() })
    }

    /// Whether `list` is a `PopulationList` key.
    pub fn has_list(&self, list: u32) -> bool {
        self.lists.contains_key(&list)
    }

    /// The units `list` can spawn (every entry; empty for an unknown list).
    pub fn units_of(&self, list: u32) -> impl Iterator<Item = u32> + '_ {
        self.entries.get(&list).into_iter().flatten().map(|e| e.unit)
    }

    /// Whether `unit` is a vehicle.
    pub fn is_vehicle(&self, unit: u32) -> bool {
        self.vehicles.contains(&unit)
    }

    /// Whether a spawner's choice is one this catalog can serve: its list is 0 (none — it spawns nothing
    /// until `TweakAttachedSpawners{SpawnList=…}` gives it one) or exists, and its weighting is a
    /// `SpawnerWeightTypeEnum` value (0..=11). An error naming what is wrong otherwise.
    pub fn check(&self, c: &UnitChoice) -> Result<(), String> {
        if c.unit_list != 0 && !self.has_list(c.unit_list) {
            return Err(format!("UnitList 0x{:08X} is no PopulationList", c.unit_list));
        }
        if c.weighting > weighting::AUTO_SQUAD_COMPOSITION {
            return Err(format!("UnitListWeighting {} is no SpawnerWeightTypeEnum value", c.weighting));
        }
        Ok(())
    }

    /// The unit a spawner with choice `c` spawns now, or `None` when its rule spawns nothing — including
    /// a chosen entry of unit 0 (lists carry some; `FUN_004E4150` spawns nothing for 0, `0x004E4209`)
    /// and list 0 (no runtime list: `FUN_004E8510` returns 0 and every rule returns 0).
    /// `owner_health` is the owner's health fraction (`FUN_004E3DD0`), read only by the `Health*` rules;
    /// `next` is the game's random draw, taken only where retail draws. Panics on a choice
    /// [`check`](Self::check) refuses — spawners are checked when registered.
    pub fn pick(&mut self, c: &UnitChoice, owner_health: impl FnOnce() -> f32, next: &mut dyn FnMut() -> u32) -> Option<u32> {
        if c.owner == 0 || c.unit_list == 0 {
            return None;
        }
        self.choose(c, owner_health, next).filter(|&u| u != 0)
    }

    fn choose(&mut self, c: &UnitChoice, owner_health: impl FnOnce() -> f32, next: &mut dyn FnMut() -> u32) -> Option<u32> {
        let list = self
            .lists
            .get(&c.unit_list)
            .unwrap_or_else(|| panic!("UnitList 0x{:08X}: spawners are checked at registration", c.unit_list));
        match c.weighting {
            weighting::NONE | weighting::AUTO_SQUAD_COMPOSITION => list.draw_uniform(next),
            weighting::SIMPLE => list.draw_weighted(next),
            weighting::EXHAUSTIVE => self.bag(c.owner, c.unit_list).draw(next),
            // `FUN_004E4150` keeps the result only if it is `> 0` as a signed int, so a template key with
            // the top bit set (every level template, `0x8…`) spawns nothing.
            weighting::EXHAUSTIVE_ONCE => self.bag(c.owner, c.unit_list).draw(next).filter(|&u| (u as i32) > 0),
            weighting::EXHAUSTIVE_GLOBAL => self.bag(c.unit_list, c.unit_list).draw(next),
            weighting::HEALTH_HIGH | weighting::HEALTH_LOW | weighting::HEALTH_BUCKET | weighting::HEALTH_BUCKET_LOW => {
                list.draw_health(c.weighting, owner_health(), next)
            }
            weighting::MINI_BOSS | weighting::MINI_BOSS_LINKED => None,
            other => panic!("UnitListWeighting {other}: spawners are checked at registration"),
        }
    }

    /// The bag under `key`, filled from `list`'s entries the first time (`FUN_004E89A0`).
    fn bag(&mut self, key: u32, list: u32) -> &mut UnitBag {
        let entries = &self.entries;
        self.bags.entry(key).or_insert_with(|| UnitBag::new(&entries[&list]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> UnitCatalog {
        let mut lists = BTreeMap::new();
        lists.insert(
            0x8000_0100,
            vec![UnitListEntry { unit: 0x8000_0001, weight: 2 }, UnitListEntry { unit: 0x8000_0002, weight: 1 }],
        );
        UnitCatalog::new(lists, BTreeSet::from([0x8000_0002])).unwrap()
    }

    #[test]
    fn simple_and_uniform_choose_from_the_list() {
        let mut c = catalog();
        let simple = UnitChoice { owner: 1, unit_list: 0x8000_0100, weighting: weighting::SIMPLE };
        // Sorted [0x…02 (w1), 0x…01 (w2)], total 3: r = 0 → 0x…02.
        assert_eq!(c.pick(&simple, || 1.0, &mut || 0), Some(0x8000_0002));
        let none = UnitChoice { weighting: weighting::NONE, ..simple };
        assert_eq!(c.pick(&none, || 1.0, &mut || 1), Some(0x8000_0001));
        assert!(c.is_vehicle(0x8000_0002));
    }

    /// `Exhaustive` keeps one bag per owner; `ExhaustiveGlobal` one per list.
    #[test]
    fn exhaustive_bags_are_per_owner_or_per_list() {
        let mut c = catalog();
        let a = UnitChoice { owner: 1, unit_list: 0x8000_0100, weighting: weighting::EXHAUSTIVE };
        let b = UnitChoice { owner: 2, ..a };
        // Data order [0x…01 ×2, 0x…02 ×1]; draw 2 → 0x…02 in each owner's own bag.
        assert_eq!(c.pick(&a, || 1.0, &mut || 2), Some(0x8000_0002));
        assert_eq!(c.pick(&b, || 1.0, &mut || 2), Some(0x8000_0002));
        let g1 = UnitChoice { owner: 1, weighting: weighting::EXHAUSTIVE_GLOBAL, ..a };
        let g2 = UnitChoice { owner: 2, ..g1 };
        assert_eq!(c.pick(&g1, || 1.0, &mut || 2), Some(0x8000_0002));
        // The shared bag lost its only 0x…02: r = 2 % 2 = 1 → 0x…01.
        assert_eq!(c.pick(&g2, || 1.0, &mut || 2), Some(0x8000_0001));
    }

    #[test]
    fn exhaustive_once_drops_level_templates_and_miniboss_spawns_nothing() {
        let mut c = catalog();
        let once = UnitChoice { owner: 1, unit_list: 0x8000_0100, weighting: weighting::EXHAUSTIVE_ONCE };
        assert_eq!(c.pick(&once, || 1.0, &mut || 0), None);
        let boss = UnitChoice { weighting: weighting::MINI_BOSS, ..once };
        assert_eq!(c.pick(&boss, || 1.0, &mut || panic!("no draw")), None);
        let ownerless = UnitChoice { owner: 0, weighting: weighting::SIMPLE, ..once };
        assert_eq!(c.pick(&ownerless, || 1.0, &mut || panic!("no draw")), None);
    }

    /// An entry of unit 0 spawns nothing.
    #[test]
    fn a_unit_zero_entry_spawns_nothing() {
        let lists = BTreeMap::from([(0x8000_0200, vec![UnitListEntry { unit: 0, weight: 1 }])]);
        let mut c = UnitCatalog::new(lists, BTreeSet::new()).unwrap();
        let choice = UnitChoice { owner: 1, unit_list: 0x8000_0200, weighting: weighting::SIMPLE };
        assert_eq!(c.pick(&choice, || 1.0, &mut || 0), None);
    }

    #[test]
    fn check_refuses_unknown_lists_and_weightings() {
        let mut c = catalog();
        assert!(c.check(&UnitChoice { owner: 1, unit_list: 0x8000_0100, weighting: 1 }).is_ok());
        // List 0 is accepted and spawns nothing, taking no draw.
        let listless = UnitChoice { owner: 1, unit_list: 0, weighting: 1 };
        assert!(c.check(&listless).is_ok());
        assert_eq!(c.pick(&listless, || 1.0, &mut || panic!("no draw")), None);
        assert!(c.check(&UnitChoice { owner: 1, unit_list: 0x8000_0999, weighting: 1 }).is_err());
        assert!(c.check(&UnitChoice { owner: 1, unit_list: 0x8000_0100, weighting: 12 }).is_err());
    }
}
