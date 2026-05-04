//! Atmosphere profile data model and palette merging.
//!
//! Profiles are loaded from JSON (`assets/rules/atmospheres.json`) using the
//! same [`load_rules_with_fallback`](crate::asset::load::load_rules_with_fallback)
//! pattern as feature/entity/scatter rules.
//!
//! # Design
//!
//! Each profile declares:
//! - **`match_tags`**: one or more tags that activate the profile. A profile
//!   activates when the room carries *any* of these tags (OR logic).
//! - **`priority`**: tie-break when the total weight budget is limited (higher = earlier).
//! - Three influence lists (scatter, features, entities), each entry carrying a
//!   **`weight`** that controls its probability of being sampled relative to
//!   other entries in the same category.
//!
//! When multiple profiles activate, their influence lists are concatenated into
//! a single [`AtmospherePalette`]. The planner then samples from each category
//! independently, using the weights as a probability distribution.

use serde::{Deserialize, Serialize};

use crate::entity::plan::EntityArchetypeId;
use crate::entity::rules::EntityPlacementStrategy;
use crate::feature::placement::PlacementStrategy;
use crate::feature::registry::FeatureType;
use crate::tag::Tag;

// ─── Influence entries ──────────────────────────────────────────────────────

/// A weighted tile-scatter influence.
///
/// Unlike a raw [`TileScatterRule`](crate::tile::scatter::TileScatterRule),
/// this carries a `weight` for probabilistic selection and a `density` that
/// controls the fraction of floor tiles replaced *if* this entry is chosen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScatterInfluence {
    /// Name of the tile to scatter (looked up in the tile registry).
    pub target_tile: String,
    /// Fraction of floor tiles to replace when this influence fires (0.0–1.0).
    pub density: f64,
    /// Relative weight for sampling. Higher = more likely to be picked.
    pub weight: f64,
}

/// A weighted feature-placement influence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureInfluence {
    /// What feature type to place.
    pub feature_type: FeatureType,
    /// How to position it inside the room.
    pub strategy: PlacementStrategy,
    /// Maximum instances per room (caps even when sampled multiple times).
    pub max_count: u32,
    /// Relative weight for sampling.
    pub weight: f64,
}

/// A weighted entity-spawn influence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityInfluence {
    /// What entity archetype to spawn.
    pub archetype: EntityArchetypeId,
    /// How to position it inside the room.
    pub placement: EntityPlacementStrategy,
    /// Maximum instances per room.
    pub max_count: u32,
    /// Relative weight for sampling.
    pub weight: f64,
}

// ─── AtmosphereProfile ──────────────────────────────────────────────────────

/// A named atmosphere profile loaded from JSON.
///
/// Activates when a room carries *any* tag in `match_tags`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtmosphereProfile {
    /// Human-readable identifier (e.g. `"damp"`, `"overgrown"`).
    pub name: String,

    /// Tags that activate this profile (OR logic — any match suffices).
    pub match_tags: Vec<Tag>,

    /// Tie-break priority when accumulating. Higher = processed first.
    /// Profiles with equal priority are all included.
    #[serde(default = "default_priority")]
    pub priority: i32,

    /// Tile scatter influences.
    #[serde(default)]
    pub scatter: Vec<ScatterInfluence>,

    /// Feature placement influences.
    #[serde(default)]
    pub features: Vec<FeatureInfluence>,

    /// Entity spawn influences.
    #[serde(default)]
    pub entities: Vec<EntityInfluence>,
}

fn default_priority() -> i32 {
    0
}

impl AtmosphereProfile {
    /// Does this profile activate for a room that carries the given tags?
    pub fn matches(&self, room_tags: &[Tag]) -> bool {
        self.match_tags.iter().any(|t| room_tags.contains(t))
    }
}

// ─── AtmospherePalette ──────────────────────────────────────────────────────

/// Merged palette produced by accumulating all active profiles for a room.
///
/// The planner samples from each category independently using the `weight`
/// fields as a probability distribution.
#[derive(Debug, Clone, Default)]
pub struct AtmospherePalette {
    /// Accumulated scatter influences from all active profiles.
    pub scatter: Vec<ScatterInfluence>,
    /// Accumulated feature influences from all active profiles.
    pub features: Vec<FeatureInfluence>,
    /// Accumulated entity influences from all active profiles.
    pub entities: Vec<EntityInfluence>,
}

impl AtmospherePalette {
    /// Is this palette empty (no active profiles matched)?
    pub fn is_empty(&self) -> bool {
        self.scatter.is_empty() && self.features.is_empty() && self.entities.is_empty()
    }

    /// Total weight of scatter influences.
    pub fn scatter_total_weight(&self) -> f64 {
        self.scatter.iter().map(|s| s.weight).sum()
    }

    /// Total weight of feature influences.
    pub fn feature_total_weight(&self) -> f64 {
        self.features.iter().map(|f| f.weight).sum()
    }

    /// Total weight of entity influences.
    pub fn entity_total_weight(&self) -> f64 {
        self.entities.iter().map(|e| e.weight).sum()
    }
}

/// Collect all profiles that match a room's tags and merge their influences
/// into a single [`AtmospherePalette`].
///
/// Profiles are sorted by descending `priority` before merging, so higher-
/// priority influences appear first in the palette (relevant when the planner
/// enforces a maximum number of samples).
pub fn build_palette(profiles: &[AtmosphereProfile], room_tags: &[Tag]) -> AtmospherePalette {
    let mut active: Vec<&AtmosphereProfile> =
        profiles.iter().filter(|p| p.matches(room_tags)).collect();
    active.sort_by(|a, b| b.priority.cmp(&a.priority));

    let mut palette = AtmospherePalette::default();
    for profile in &active {
        palette.scatter.extend(profile.scatter.iter().cloned());
        palette.features.extend(profile.features.iter().cloned());
        palette.entities.extend(profile.entities.iter().cloned());
    }
    palette
}

/// Load the default atmosphere profiles.
///
/// Follows the standard asset-loading pattern: tries
/// `assets/rules/atmospheres.json` on disk first, falls back to the
/// compiled-in version if the file doesn't exist.
pub fn default_profiles() -> Vec<AtmosphereProfile> {
    crate::asset::load::load_default_atmosphere_profiles()
        .expect("embedded atmosphere profiles are valid JSON (compile-time guarantee)")
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn damp_profile() -> AtmosphereProfile {
        AtmosphereProfile {
            name: "damp".into(),
            match_tags: vec![Tag::from("damp"), Tag::from("flooded")],
            priority: 0,
            scatter: vec![ScatterInfluence {
                target_tile: "water".into(),
                density: 0.15,
                weight: 3.0,
            }],
            features: vec![FeatureInfluence {
                feature_type: FeatureType::from("moss"),
                strategy: PlacementStrategy::RandomFloor,
                max_count: 3,
                weight: 2.0,
            }],
            entities: vec![EntityInfluence {
                archetype: EntityArchetypeId::from("rat"),
                placement: EntityPlacementStrategy::RandomFloor,
                max_count: 2,
                weight: 1.0,
            }],
        }
    }

    fn overgrown_profile() -> AtmosphereProfile {
        AtmosphereProfile {
            name: "overgrown".into(),
            match_tags: vec![Tag::from("overgrown"), Tag::from("natural")],
            priority: 0,
            scatter: vec![ScatterInfluence {
                target_tile: "grass".into(),
                density: 0.25,
                weight: 4.0,
            }],
            features: vec![
                FeatureInfluence {
                    feature_type: FeatureType::from("vines"),
                    strategy: PlacementStrategy::WallAdjacent,
                    max_count: 3,
                    weight: 3.0,
                },
                FeatureInfluence {
                    feature_type: FeatureType::from("fungus"),
                    strategy: PlacementStrategy::Corner,
                    max_count: 2,
                    weight: 1.0,
                },
            ],
            entities: vec![],
        }
    }

    #[test]
    fn profile_matches_any_tag() {
        let profile = damp_profile();
        assert!(profile.matches(&[Tag::from("damp")]));
        assert!(profile.matches(&[Tag::from("flooded")]));
        assert!(profile.matches(&[Tag::from("noble"), Tag::from("damp")]));
        assert!(!profile.matches(&[Tag::from("dry")]));
        assert!(!profile.matches(&[]));
    }

    #[test]
    fn build_palette_merges_multiple_profiles() {
        let profiles = vec![damp_profile(), overgrown_profile()];
        let tags = vec![Tag::from("damp"), Tag::from("overgrown")];

        let palette = build_palette(&profiles, &tags);

        // Scatter: 1 from damp + 1 from overgrown = 2
        assert_eq!(palette.scatter.len(), 2);
        // Features: 1 from damp + 2 from overgrown = 3
        assert_eq!(palette.features.len(), 3);
        // Entities: 1 from damp + 0 from overgrown = 1
        assert_eq!(palette.entities.len(), 1);
    }

    #[test]
    fn build_palette_empty_when_no_tags_match() {
        let profiles = vec![damp_profile(), overgrown_profile()];
        let tags = vec![Tag::from("noble"), Tag::from("sealed")];

        let palette = build_palette(&profiles, &tags);
        assert!(palette.is_empty());
    }

    #[test]
    fn build_palette_respects_priority_order() {
        let mut high = damp_profile();
        high.priority = 10;
        let mut low = overgrown_profile();
        low.priority = 1;
        // Both match "natural" + "damp"
        low.match_tags.push(Tag::from("damp"));

        let profiles = vec![low.clone(), high.clone()]; // inserted in wrong order
        let tags = vec![Tag::from("damp")];

        let palette = build_palette(&profiles, &tags);

        // High-priority (damp, pri=10) scatter comes first.
        assert_eq!(palette.scatter[0].target_tile, "water");
        assert_eq!(palette.scatter[1].target_tile, "grass");
    }

    #[test]
    fn palette_total_weight_sums_correctly() {
        let profiles = vec![damp_profile(), overgrown_profile()];
        let tags = vec![Tag::from("damp"), Tag::from("overgrown")];

        let palette = build_palette(&profiles, &tags);

        assert!((palette.scatter_total_weight() - 7.0).abs() < f64::EPSILON); // 3.0 + 4.0
        assert!((palette.feature_total_weight() - 6.0).abs() < f64::EPSILON); // 2.0 + 3.0 + 1.0
        assert!((palette.entity_total_weight() - 1.0).abs() < f64::EPSILON); // 1.0
    }

    #[test]
    fn serde_round_trip_profile() {
        let profile = damp_profile();
        let json = serde_json::to_string_pretty(&profile).unwrap();
        let deser: AtmosphereProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, profile.name);
        assert_eq!(deser.match_tags, profile.match_tags);
        assert_eq!(deser.scatter.len(), profile.scatter.len());
        assert_eq!(deser.features.len(), profile.features.len());
        assert_eq!(deser.entities.len(), profile.entities.len());
    }

    #[test]
    fn serde_round_trip_vec_of_profiles() {
        let profiles = vec![damp_profile(), overgrown_profile()];
        let json = serde_json::to_string_pretty(&profiles).unwrap();
        let deser: Vec<AtmosphereProfile> = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.len(), 2);
        assert_eq!(deser[0].name, "damp");
        assert_eq!(deser[1].name, "overgrown");
    }

    #[test]
    fn serde_default_fields() {
        // Minimal JSON — scatter/features/entities default to [], priority to 0.
        let json = r#"{"name":"bare","match_tags":["test"]}"#;
        let profile: AtmosphereProfile = serde_json::from_str(json).unwrap();
        assert_eq!(profile.priority, 0);
        assert!(profile.scatter.is_empty());
        assert!(profile.features.is_empty());
        assert!(profile.entities.is_empty());
    }
}
