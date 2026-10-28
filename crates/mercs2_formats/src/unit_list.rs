//! Population unit lists — what a spawner spawns, as retail chooses it.
//!
//! **Data.** A `PopulationSimpleSpawner` names a list in its `UnitList` field (`+0x2C`) and a choice
//! rule in `UnitListWeighting` (`+0x48`, `SpawnerWeightTypeEnum`). The list is every
//! `PopulationList` record (`{UnitId, ProbabilityWeight}`) keyed to that list's template key, e.g.
//! "Spawnlist (Guerilla Ground)" `0x8000720A` → five guerrilla templates weighted 4/2/2/1/2.
//!
//! **Runtime list** ([`RuntimeUnitList`], `FUN_004E83F0` → `FUN_004E8130`): the list's entries in data
//! order (at most 64), sorted ascending by weight by `FUN_004DC870`, with
//! `total = Σ max(weight, 1)`. The sort is an iterative quicksort (stack of `(hi, lo)` pairs, Hoare
//! partition on the middle element, left range first) that hands ranges shorter than 8 to an
//! insertion sort, so equal weights keep data order only on short lists. Checked against all 30
//! runtime lists of the four `updated-chris-exports` captures.
//!
//! **Choice** (`FUN_004E4150`, switch on `UnitListWeighting`, jump table `0x004E4388`). Each rule
//! takes its draws from `next` — the game's random state (`(x & 0xFFFF0000) | (u >> 16)`,
//! `mercs2_core::random::Lcg::next_draw`) — only where retail draws:
//!
//! | `SpawnerWeightTypeEnum` | rule |
//! |---|---|
//! | `Simple` (1) | [`RuntimeUnitList::draw_weighted`] (`0x0247AB90`) |
//! | `None` (0), `AutoSquadComposition` (11) | [`RuntimeUnitList::draw_uniform`] (`FUN_004E8730`) |
//! | `Exhaustive` (2), `ExhaustiveOnce` (3), `ExhaustiveGlobal` (4) | [`UnitBag::draw`] (`FUN_004E8AB0`) |
//! | `HealthHigh`, `HealthLow`, `HealthBucket`, `HealthBucketLow` (5–8) | [`RuntimeUnitList::draw_health`] (`FUN_004E8ED0`) |
//! | `MiniBoss`, `MiniBossLinked` (9, 10) | spawns nothing |

use std::collections::BTreeMap;

use crate::worldentity::{Payload, WorldEntity};

/// One `PopulationList` entry: a template and its `ProbabilityWeight`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitListEntry {
    pub unit: u32,
    pub weight: i32,
}

/// `FUN_004E83F0` builds a runtime list from at most this many entries.
pub const MAX_LIST_ENTRIES: usize = 64;

/// The `SpawnerWeightTypeEnum` values `UnitListWeighting` takes.
pub mod weighting {
    pub const NONE: u32 = 0;
    pub const SIMPLE: u32 = 1;
    pub const EXHAUSTIVE: u32 = 2;
    pub const EXHAUSTIVE_ONCE: u32 = 3;
    pub const EXHAUSTIVE_GLOBAL: u32 = 4;
    pub const HEALTH_HIGH: u32 = 5;
    pub const HEALTH_LOW: u32 = 6;
    pub const HEALTH_BUCKET: u32 = 7;
    pub const HEALTH_BUCKET_LOW: u32 = 8;
    pub const MINI_BOSS: u32 = 9;
    pub const MINI_BOSS_LINKED: u32 = 10;
    pub const AUTO_SQUAD_COMPOSITION: u32 = 11;
}

/// The health-weighted rules compare each weight with `health × 100` (`DAT_00B92870`).
const HEALTH_SCALE: f32 = 100.0;

fn r_u32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

/// Every unit list of the container: list template key → its entries in data order. An error when a
/// `PopulationList` payload is not `{u32, i32}`.
pub fn unit_lists(we: &WorldEntity) -> Result<BTreeMap<u32, Vec<UnitListEntry>>, String> {
    let mut out: BTreeMap<u32, Vec<UnitListEntry>> = BTreeMap::new();
    for c in we.groups_of("PopulationList") {
        for r in &c.records {
            let e = match &r.payload {
                Payload::Fixed(b) if b.len() == 8 => UnitListEntry { unit: r_u32(b, 0), weight: r_u32(b, 4) as i32 },
                p => return Err(format!("PopulationList: payload {p:?} is not {{UnitId, ProbabilityWeight}}")),
            };
            for &k in &r.keys {
                out.entry(k).or_default().push(e);
            }
        }
    }
    Ok(out)
}

/// A list as `FUN_004E8130` holds it: units and raw weights sorted ascending by weight (retail's
/// sort, module docs), and `total = Σ max(weight, 1)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeUnitList {
    pub units: Vec<u32>,
    pub weights: Vec<i32>,
    pub total: u32,
}

impl RuntimeUnitList {
    /// Build from a list's entries in data order. An error past [`MAX_LIST_ENTRIES`] — retail
    /// would silently use only the first 64.
    pub fn build(entries: &[UnitListEntry]) -> Result<RuntimeUnitList, String> {
        if entries.len() > MAX_LIST_ENTRIES {
            return Err(format!("{} entries; retail builds a list from at most {MAX_LIST_ENTRIES}", entries.len()));
        }
        let mut units: Vec<u32> = entries.iter().map(|e| e.unit).collect();
        let mut weights: Vec<i32> = entries.iter().map(|e| e.weight).collect();
        retail_sort(&mut weights, &mut units);
        let total = weights.iter().map(|&w| w.max(1) as u32).sum();
        Ok(RuntimeUnitList { units, weights, total })
    }

    /// `Simple` (`0x0247AB90`): `r = draw % total`; the first unit whose running sum of
    /// `max(weight, 1)` is `>= r`, else the last. (`>=`, not `>`: the first unit gets one extra
    /// chance — retail's arithmetic.) `None` for an empty list, which takes no draw.
    pub fn draw_weighted(&self, next: &mut dyn FnMut() -> u32) -> Option<u32> {
        if self.units.is_empty() {
            return None;
        }
        let r = next() % self.total;
        let mut cum = 0u32;
        for (i, &w) in self.weights.iter().enumerate() {
            cum += w.max(1) as u32;
            if cum >= r {
                return Some(self.units[i]);
            }
        }
        self.units.last().copied()
    }

    /// `None` / `AutoSquadComposition` (`FUN_004E8730`): `units[draw % n]` — weights ignored. `None`
    /// for an empty list, which takes no draw.
    pub fn draw_uniform(&self, next: &mut dyn FnMut() -> u32) -> Option<u32> {
        if self.units.is_empty() {
            return None;
        }
        Some(self.units[(next() % self.units.len() as u32) as usize])
    }

    /// The health-weighted rules (`FUN_004E8ED0` → `FUN_004E88B0` / `FUN_004E8920` / `FUN_004E8790` /
    /// `FUN_004E8820`). The weights act as thresholds against `t = health × 100`, `health` being the
    /// spawner owner's health fraction in `0..=1`; `weighting` is one of the four `HEALTH_*` values.
    ///
    /// - `HealthHigh`: `c` = the last index whose weight `<= t`; `c == 0` → that unit, else one of
    ///   the units **below** `c`, `units[draw % c]`.
    /// - `HealthLow`: the `k` units whose weight `> t`; one of them, `units[n - 1 - draw % k]`.
    /// - `HealthBucket`: `c` as for `HealthHigh`; `c == 0` → that unit; else one of the run of equal
    ///   weights ending at `c` (that unit alone when the run is one long).
    /// - `HealthBucketLow`: `l` = the first index whose weight `> t`; `l == n - 1` → that unit; else
    ///   one of the run of equal weights starting at `l` (that unit alone when the run is one long).
    ///
    /// `None` (no draw) when no weight is on the needed side of `t`, and for an empty list. Weights and
    /// `t` compare as `f32`, as retail converts them.
    pub fn draw_health(&self, weighting_rule: u32, health: f32, next: &mut dyn FnMut() -> u32) -> Option<u32> {
        let n = self.units.len();
        if n == 0 {
            return None;
        }
        let t = health * HEALTH_SCALE;
        let w = |i: usize| self.weights[i] as f32;
        let last_at_or_below = || (0..n).rev().find(|&i| w(i) <= t);
        let pick = |i: usize| Some(self.units[i]);
        match weighting_rule {
            weighting::HEALTH_HIGH => match last_at_or_below()? {
                0 => pick(0),
                c => pick((next() % c as u32) as usize),
            },
            weighting::HEALTH_LOW => {
                let k = (0..n).rev().take_while(|&i| w(i) > t).count();
                if k == 0 {
                    return None;
                }
                pick(n - 1 - (next() % k as u32) as usize)
            }
            weighting::HEALTH_BUCKET => match last_at_or_below()? {
                0 => pick(0),
                c => {
                    let lo = (0..c).rev().take_while(|&i| self.weights[i] == self.weights[c]).last().unwrap_or(c);
                    match c - lo + 1 {
                        1 => pick(c),
                        count => pick(lo + (next() % count as u32) as usize),
                    }
                }
            },
            weighting::HEALTH_BUCKET_LOW => {
                let l = (0..n).find(|&i| t < w(i))?;
                if l == n - 1 {
                    return pick(l);
                }
                let hi = (l + 1..n).take_while(|&i| self.weights[i] == self.weights[l]).last().unwrap_or(l);
                match hi - l + 1 {
                    1 => pick(l),
                    count => pick(l + (next() % count as u32) as usize),
                }
            }
            other => panic!("draw_health: weighting {other} is not a HEALTH_* rule"),
        }
    }
}

/// `FUN_004DC870`: sort `keys` ascending, carrying `values` along, exactly as retail orders ties.
fn retail_sort(keys: &mut [i32], values: &mut [u32]) {
    let n = keys.len();
    if n <= 1 {
        return;
    }
    // (hi, lo) pairs; the left range is pushed last so it is sorted first.
    let mut stack: Vec<(usize, usize)> = vec![(n - 1, 0)];
    while let Some((hi, lo)) = stack.pop() {
        let size = hi - lo + 1;
        if size < 8 {
            insertion_sort(&mut keys[lo..=hi], &mut values[lo..=hi]);
            continue;
        }
        let pivot = keys[(lo + hi) >> 1];
        let (mut i, mut j) = (lo as isize, hi as isize);
        while i <= j {
            while keys[i as usize] < pivot {
                i += 1;
            }
            while keys[j as usize] > pivot {
                j -= 1;
            }
            if i <= j {
                keys.swap(i as usize, j as usize);
                values.swap(i as usize, j as usize);
                i += 1;
                j -= 1;
            }
        }
        if (i as usize) < hi {
            stack.push((hi, i as usize));
        }
        if (lo as isize) < j {
            stack.push((j as usize, lo));
        }
    }
}

/// The small-range sort: insertion, shifting only strictly greater keys (stable).
fn insertion_sort(keys: &mut [i32], values: &mut [u32]) {
    for i in 1..keys.len() {
        let (k, v) = (keys[i], values[i]);
        let mut j = i;
        while j > 0 && keys[j - 1] > k {
            keys[j] = keys[j - 1];
            values[j] = values[j - 1];
            j -= 1;
        }
        keys[j] = k;
        values[j] = v;
    }
}

/// An `Exhaustive*` bag (`FUN_004E89A0` fills it, `FUN_004E8AB0` draws): one slot per list entry in
/// data order, each holding `max(weight, 1)` draws. A draw removes one; an empty bag refills.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitBag {
    units: Vec<u32>,
    remaining: Vec<u32>,
    initial: Vec<u32>,
}

impl UnitBag {
    pub fn new(entries: &[UnitListEntry]) -> UnitBag {
        let initial: Vec<u32> = entries.iter().map(|e| e.weight.max(1) as u32).collect();
        UnitBag { units: entries.iter().map(|e| e.unit).collect(), remaining: initial.clone(), initial }
    }

    /// The draws left before the bag refills.
    pub fn remaining(&self) -> u32 {
        self.remaining.iter().sum()
    }

    /// `r = draw % remaining`; the first slot whose running sum of remaining counts exceeds `r` gives
    /// its unit and loses one count. An empty bag refills to its initial counts first. `None` for a
    /// bag with no slots, which takes no draw.
    pub fn draw(&mut self, next: &mut dyn FnMut() -> u32) -> Option<u32> {
        if self.remaining() == 0 {
            self.remaining.clone_from(&self.initial);
        }
        let total = self.remaining();
        if total == 0 {
            return None;
        }
        let r = next() % total;
        let mut cum = 0u32;
        for i in 0..self.units.len() {
            cum += self.remaining[i];
            if r < cum {
                self.remaining[i] -= 1;
                return Some(self.units[i]);
            }
        }
        unreachable!("r < total = Σ remaining");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(v: &[(u32, i32)]) -> Vec<UnitListEntry> {
        v.iter().map(|&(unit, weight)| UnitListEntry { unit, weight }).collect()
    }

    /// Short lists keep data order among equal weights; the total counts zero weights as one.
    #[test]
    fn short_lists_sort_stably_and_total_counts_zero_as_one() {
        let l = RuntimeUnitList::build(&entries(&[(1, 8), (2, 3), (3, 8), (4, 3), (5, 0)])).unwrap();
        assert_eq!(l.units, vec![5, 2, 4, 1, 3]);
        assert_eq!(l.weights, vec![0, 3, 3, 8, 8]);
        assert_eq!(l.total, 1 + 3 + 3 + 8 + 8);
    }

    /// A live list of 11 entries (`0x80009D17`, capture `20261021_chris-jacobs-mid-mission`): the
    /// quicksort reorders equal weights away from data order.
    #[test]
    fn long_lists_sort_like_the_live_capture() {
        let data = entries(&[
            (0x80009CC4, 19),
            (0x80009E04, 20),
            (0x8000AC48, 18),
            (0x8000AC50, 16),
            (0x80009CC7, 18),
            (0x80009E03, 20),
            (0x8000AC45, 21),
            (0x8000AC49, 20),
            (0x8000AC4B, 16),
            (0x8000AC4F, 20),
            (0x8000AC4A, 16),
        ]);
        let l = RuntimeUnitList::build(&data).unwrap();
        assert_eq!(
            l.units,
            vec![
                0x8000AC4A, 0x8000AC50, 0x8000AC4B, 0x8000AC48, 0x80009CC7, 0x80009CC4, 0x8000AC4F, 0x8000AC49,
                0x80009E03, 0x80009E04, 0x8000AC45
            ]
        );
        assert_eq!(l.total, 204);
    }

    /// A draw source that hands out `draws` in order and counts how many were taken.
    fn source(draws: &[u32]) -> (impl FnMut() -> u32 + '_, std::rc::Rc<std::cell::Cell<usize>>) {
        let taken = std::rc::Rc::new(std::cell::Cell::new(0));
        let t = taken.clone();
        (
            move || {
                let d = draws[t.get()];
                t.set(t.get() + 1);
                d
            },
            taken,
        )
    }

    #[test]
    fn weighted_draw_uses_greater_or_equal() {
        let l = RuntimeUnitList::build(&entries(&[(1, 1), (2, 2)])).unwrap();
        // total 3; running sums 1, 3. r=0 → 1, r=1 → 1 (1 >= 1), r=2 → 2.
        for (draw, unit) in [(0, 1), (1, 1), (2, 2), (5, 2)] {
            assert_eq!(l.draw_weighted(&mut || draw), Some(unit), "draw {draw}");
        }
        let (mut next, taken) = source(&[7]);
        assert_eq!(RuntimeUnitList::build(&[]).unwrap().draw_weighted(&mut next), None);
        assert_eq!(taken.get(), 0, "an empty list takes no draw");
    }

    #[test]
    fn uniform_draw_ignores_weights() {
        let l = RuntimeUnitList::build(&entries(&[(1, 100), (2, 1)])).unwrap();
        assert_eq!(l.draw_uniform(&mut || 0), Some(2)); // sorted: [2 (w1), 1 (w100)]
        assert_eq!(l.draw_uniform(&mut || 1), Some(1));
    }

    /// The bag hands out each unit `max(weight, 1)` times, then refills.
    #[test]
    fn bag_exhausts_then_refills() {
        let mut b = UnitBag::new(&entries(&[(1, 2), (2, 0)]));
        assert_eq!(b.remaining(), 3);
        assert_eq!(b.draw(&mut || 0), Some(1));
        assert_eq!(b.draw(&mut || 0), Some(1));
        assert_eq!(b.draw(&mut || 0), Some(2));
        assert_eq!(b.remaining(), 0);
        assert_eq!(b.draw(&mut || 2), Some(2)); // refilled to [2, 1]: r = 2 % 3 = 2 → slot 2
        assert_eq!(b.remaining(), 2);
    }

    /// The health rules read the sorted weights as thresholds against `health × 100`, and draw only
    /// where retail does.
    #[test]
    fn health_rules_pick_by_threshold() {
        use weighting::*;
        // Sorted weights 0, 50, 50, 75, 100 (units 10, 20, 30, 40, 50).
        let l = RuntimeUnitList::build(&entries(&[(50, 100), (20, 50), (10, 0), (40, 75), (30, 50)])).unwrap();
        assert_eq!(l.weights, vec![0, 50, 50, 75, 100]);

        // health 0.6 → t 60: last weight <= 60 is index 2 (50); first weight > 60 is index 3 (75).
        // HealthHigh: one of units[0..2] → draw % 2.
        assert_eq!(l.draw_health(HEALTH_HIGH, 0.6, &mut || 3), Some(20));
        // HealthLow: the 2 units above 60 → units[4 - draw % 2].
        assert_eq!(l.draw_health(HEALTH_LOW, 0.6, &mut || 1), Some(40));
        // HealthBucket: the run of 50s ending at index 2 = indices 1..=2.
        assert_eq!(l.draw_health(HEALTH_BUCKET, 0.6, &mut || 0), Some(20));
        assert_eq!(l.draw_health(HEALTH_BUCKET, 0.6, &mut || 1), Some(30));
        // HealthBucketLow: first weight > 60 is index 3 (run of one) → that unit, no draw.
        let (mut next, taken) = source(&[]);
        assert_eq!(l.draw_health(HEALTH_BUCKET_LOW, 0.6, &mut next), Some(40));
        assert_eq!(taken.get(), 0);

        // health 0 → t 0: only index 0 is <= 0 → HealthHigh / HealthBucket return it without a draw.
        let (mut next, taken) = source(&[]);
        assert_eq!(l.draw_health(HEALTH_HIGH, 0.0, &mut next), Some(10));
        assert_eq!(l.draw_health(HEALTH_BUCKET, 0.0, &mut next), Some(10));
        assert_eq!(taken.get(), 0);
        // health 1 → t 100: nothing is > 100 → HealthLow / HealthBucketLow spawn nothing.
        assert_eq!(l.draw_health(HEALTH_LOW, 1.0, &mut || 0), None);
        assert_eq!(l.draw_health(HEALTH_BUCKET_LOW, 1.0, &mut || 0), None);
    }

    #[test]
    fn lists_past_64_entries_are_refused() {
        let many: Vec<UnitListEntry> = (0..65).map(|i| UnitListEntry { unit: i, weight: 1 }).collect();
        assert!(RuntimeUnitList::build(&many).is_err());
    }
}
