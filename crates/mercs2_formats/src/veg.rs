//! Vegetation classifier — the "is this an instanceable vegetation mesh" predicate for the
//! foliage-instancing project (`docs/reverse_engineer/foliage_instancing_plan.md` §7).
//!
//! Core rule: classify on the **descriptor token**, never the region prefix. `jungle_env_*` also
//! names `jungle_env_rock02`, so a prefix match sweeps in rocks. Exclusions (shadow-blob decals,
//! rocks, road medians, jungle walls, and substring-collision guards like `street`→`tree`) run
//! FIRST, then positive descriptor tokens most-specific-first.

/// Coarse vegetation category — for census grouping and later per-class LOD/residency tuning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VegClass {
    Palm,
    Tree,
    Bush,
    Shrub,
    Plant,
    Vine,
    Canopy,
    Fern,
    Hedge,
    Grass,
    Flower,
    /// Standalone leaf-card sub-mesh (a `*_leaves` name not already caught by tree/bush/hedge/…).
    Leaves,
    /// Standalone branch/trunk sub-mesh (a `*branch`/`*trunk` name not caught by a fuller token).
    Branch,
    /// Generic `*foliage*` catch-all.
    Foliage,
}

impl VegClass {
    pub fn label(self) -> &'static str {
        match self {
            VegClass::Palm => "palm",
            VegClass::Tree => "tree",
            VegClass::Bush => "bush",
            VegClass::Shrub => "shrub",
            VegClass::Plant => "plant",
            VegClass::Vine => "vine",
            VegClass::Canopy => "canopy",
            VegClass::Fern => "fern",
            VegClass::Hedge => "hedge",
            VegClass::Grass => "grass",
            VegClass::Flower => "flower",
            VegClass::Leaves => "leaves",
            VegClass::Branch => "branch",
            VegClass::Foliage => "foliage",
        }
    }
}

/// A classified vegetation placement descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VegTag {
    pub class: VegClass,
    /// A distant billboard-imposter variant (`*_imposter` / `*_imposter_dm`). These are in-set — the
    /// instanced path carries the mesh↔imposter crossfade — but callers may want to treat the LOD
    /// tiers differently.
    pub imposter: bool,
}

/// Substrings that, if present, disqualify a name even though a positive token might also match.
/// - `fakeshadow`/`decal`: projected/blob shadow decals, not geometry.
/// - `rock`: rock props (swept in only via the region prefix).
/// - `grassymedian`: road-median geometry, terrain-adjacent.
/// - `junglewall`: a large jungle-wall mesh, not a discrete instanceable tree.
/// - `street`/`ravine`/`ambush`: substring-collision guards (`street`⊃`tree`, `ravine`⊃`vine`,
///   `ambush`⊃`bush`).
const EXCLUDE: &[&str] = &[
    "fakeshadow", "decal", "rock", "grassymedian", "junglewall", "street", "ravine", "ambush",
];

/// Returns `Some(VegTag)` when `base_name` names an instanceable vegetation mesh, else `None`.
/// `base_name` is the placement base name with any leading `_` already stripped (matching how
/// `placement::Placement::name` / the name-hash resolution treat it).
pub fn classify(base_name: &str) -> Option<VegTag> {
    let n = base_name.to_ascii_lowercase();

    if EXCLUDE.iter().any(|t| n.contains(t)) {
        return None;
    }

    let imposter = n.contains("imposter");

    // Positive descriptor tokens, most-specific first (palm before tree; grass/lawn share a bucket).
    let class = if n.contains("palm") {
        VegClass::Palm
    } else if n.contains("tree") {
        VegClass::Tree
    } else if n.contains("bush") {
        VegClass::Bush
    } else if n.contains("shrub") {
        VegClass::Shrub
    } else if n.contains("canopy") {
        VegClass::Canopy
    } else if n.contains("hedge") {
        VegClass::Hedge
    } else if n.contains("fern") {
        VegClass::Fern
    } else if n.contains("vine") {
        VegClass::Vine
    } else if n.contains("plant") {
        VegClass::Plant
    } else if n.contains("flower") {
        VegClass::Flower
    } else if n.contains("grass") || n.contains("lawngrass") {
        VegClass::Grass
    } else if n.contains("leaves") || n.contains("leaf") {
        VegClass::Leaves
    } else if n.contains("branch") || n.contains("trunk") {
        VegClass::Branch
    } else if n.contains("foliage") {
        VegClass::Foliage
    } else {
        return None;
    };

    Some(VegTag { class, imposter })
}

/// Convenience: just the predicate.
pub fn is_vegetation(base_name: &str) -> bool {
    classify(base_name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positives_classify() {
        assert_eq!(classify("jungle_env_treesmall01").unwrap().class, VegClass::Tree);
        assert_eq!(classify("global_env_palmtreebend02").unwrap().class, VegClass::Palm);
        assert_eq!(classify("jungle_env_bushsmall03").unwrap().class, VegClass::Bush);
        assert_eq!(classify("jungle_env_plantlarge04").unwrap().class, VegClass::Plant);
        assert_eq!(classify("global_scrub_grassgreen01").unwrap().class, VegClass::Grass);
        assert_eq!(classify("global_scrub_fern01").unwrap().class, VegClass::Fern);
        assert_eq!(classify("global_env_hedgeshort").unwrap().class, VegClass::Hedge);
        assert_eq!(classify("jungle_env_vines01").unwrap().class, VegClass::Vine);
        assert_eq!(classify("jungle_env_largecanopy01").unwrap().class, VegClass::Canopy);
        assert_eq!(classify("jungle_env_leaves01").unwrap().class, VegClass::Leaves);
        // real tree by a sidewalk — must survive the `street` guard (it doesn't contain "street").
        assert_eq!(classify("global_env_treesidewalk01").unwrap().class, VegClass::Tree);
    }

    #[test]
    fn imposter_flagged() {
        let t = classify("jungle_env_treesmall01_imposter").unwrap();
        assert_eq!(t.class, VegClass::Tree);
        assert!(t.imposter);
        assert!(!classify("jungle_env_treesmall01").unwrap().imposter);
    }

    #[test]
    fn negatives_rejected() {
        // region-prefix sweep-ins and non-geometry:
        assert!(classify("jungle_env_rock02").is_none());
        assert!(classify("global_env_palmtree01_fakeshadow").is_none());
        assert!(classify("jungle_env_canopyfakeshadow").is_none());
        assert!(classify("global_decal_grass_bullet_01").is_none());
        assert!(classify("commercial_grassymedian_straight").is_none());
        assert!(classify("jungle_env_junglewall").is_none());
        // substring-collision guards:
        assert!(classify("global_streetlamp01").is_none());
        assert!(classify("mountain_env_ravine01").is_none());
        // unrelated props:
        assert!(classify("global_lamppostA").is_none());
        assert!(classify("global_chaircafe").is_none());
    }
}
