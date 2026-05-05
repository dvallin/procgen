//! Atmosphere influence application — samples from a merged [`AtmospherePalette`]
//! to produce concrete scatter rules, feature rules, and entity rules for a room.
//!
//! This is the integration bridge between the atmosphere system and the existing
//! planners. Rather than replacing the planners, it *feeds* them: the sampled
//! influences are converted into the same [`FeatureRule`], [`EntityRule`], and
//! [`TileScatterRule`] types that the planners already consume.

use rand::Rng;
use tracing::debug;

use crate::entity::rules::EntityRule;
use crate::feature::rules::FeatureRule;
use crate::spatial::plan::SpaceSpec;
use crate::tag::Tag;
use crate::tile::scatter::TileScatterRule;

use super::profile::{
    AtmospherePalette, AtmosphereProfile, EntityInfluence, FeatureInfluence, build_palette,
};

/// Results of applying atmosphere influences for a single room.
///
/// These are additive — the planner should merge them with any rules that
/// already matched via the traditional role/archetype/tag system.
#[derive(Debug, Clone, Default)]
pub struct AtmosphereContributions {
    /// Extra scatter rules to apply during tile scatter.
    pub scatter_rules: Vec<TileScatterRule>,
    /// Extra feature rules to evaluate during feature planning.
    pub feature_rules: Vec<FeatureRule>,
    /// Extra entity rules to evaluate during entity planning.
    pub entity_rules: Vec<EntityRule>,
}

/// Apply atmosphere profiles to a single room, producing concrete rules.
///
/// 1. Builds a merged [`AtmospherePalette`] from all profiles that match
///    the room's tags.
/// 2. For each category (scatter / features / entities), performs
///    `sample_budget` weighted random draws from the palette.
/// 3. Converts each sampled influence into the corresponding rule type
///    that the existing planners understand.
///
/// # Arguments
///
/// * `profiles` — All loaded atmosphere profiles.
/// * `spec` — The [`SpaceSpec`] for the room being planned.
/// * `sample_budget` — How many weighted draws to make per category.
///   A typical value is 2–4. Higher budgets produce denser, more varied rooms.
/// * `rng` — Seeded RNG for deterministic sampling.
///
/// # Returns
///
/// An [`AtmosphereContributions`] containing the sampled rules, or an empty
/// contributions struct if no profiles matched.
pub fn apply_atmosphere_influences(
    profiles: &[AtmosphereProfile],
    spec: &SpaceSpec,
    sample_budget: usize,
    rng: &mut impl Rng,
) -> AtmosphereContributions {
    let palette = build_palette(profiles, &spec.atmosphere_tags);

    if palette.is_empty() {
        debug!(
            space_id = spec.id.0,
            label = ?spec.label,
            atmosphere_tags = ?spec.atmosphere_tags,
            "no atmosphere profiles matched"
        );
        return AtmosphereContributions::default();
    }

    debug!(
        space_id = spec.id.0,
        scatter_choices = palette.scatter.len(),
        feature_choices = palette.features.len(),
        entity_choices = palette.entities.len(),
        "atmosphere palette built"
    );

    let scatter_rules = collect_scatter(&palette, &spec.atmosphere_tags);
    let feature_rules = sample_features(&palette, sample_budget, rng);
    let entity_rules = sample_entities(&palette, sample_budget, rng);

    debug!(
        space_id = spec.id.0,
        scatter = scatter_rules.len(),
        sampled_features = feature_rules.len(),
        sampled_entities = entity_rules.len(),
        "atmosphere influences applied"
    );

    AtmosphereContributions {
        scatter_rules,
        feature_rules,
        entity_rules,
    }
}

// ─── Weighted sampling helpers ──────────────────────────────────────────────

/// Perform a single weighted random selection from a slice of (item, weight) pairs.
/// Returns `None` if the slice is empty or total weight is zero.
fn weighted_sample<'a, T>(items: &'a [(T, f64)], rng: &mut impl Rng) -> Option<&'a T> {
    let total: f64 = items.iter().map(|(_, w)| w).sum();
    if total <= 0.0 || items.is_empty() {
        return None;
    }

    let mut roll: f64 = rng.r#gen::<f64>() * total;
    for (item, weight) in items {
        roll -= weight;
        if roll <= 0.0 {
            return Some(item);
        }
    }
    // Floating-point edge case: return last item.
    Some(&items.last().unwrap().0)
}

/// Collect ALL scatter influences from the palette — scatter is deterministic,
/// not sampled. If a profile matches, all its scatter entries fire (tiles are
/// the ground itself, not random decoration).
///
/// When multiple profiles contribute the same target tile, the highest density
/// is kept (no double-application).
fn collect_scatter(palette: &AtmospherePalette, room_tags: &[Tag]) -> Vec<TileScatterRule> {
    if palette.scatter.is_empty() {
        return vec![];
    }

    // Use the first room tag as the match_tag (the scatter system needs one).
    // This is a synthetic tag — the scatter rule is already known to apply.
    let match_tag = room_tags
        .first()
        .cloned()
        .unwrap_or_else(|| Tag::from("atmosphere"));

    let mut results: Vec<TileScatterRule> = palette
        .scatter
        .iter()
        .map(|influence| TileScatterRule {
            target_tile: influence.target_tile.clone(),
            density: influence.density,
            match_tag: match_tag.clone(),
        })
        .collect();

    // Deduplicate by target_tile — keep the highest density for each.
    results.sort_by(|a, b| a.target_tile.cmp(&b.target_tile));
    results.dedup_by(|a, b| {
        if a.target_tile == b.target_tile {
            b.density = b.density.max(a.density);
            true
        } else {
            false
        }
    });
    results
}

fn sample_features(
    palette: &AtmospherePalette,
    budget: usize,
    rng: &mut impl Rng,
) -> Vec<FeatureRule> {
    if palette.features.is_empty() {
        return vec![];
    }

    let weighted: Vec<(&FeatureInfluence, f64)> =
        palette.features.iter().map(|f| (f, f.weight)).collect();

    let mut results = Vec::new();
    for _ in 0..budget {
        if let Some(influence) = weighted_sample(&weighted, rng) {
            // Only add if we haven't already added this feature type.
            if !results
                .iter()
                .any(|r: &FeatureRule| r.feature_type == influence.feature_type)
            {
                results.push(FeatureRule {
                    feature_type: influence.feature_type.clone(),
                    strategy: influence.strategy,
                    required: false, // atmosphere features are never required
                    max_count: influence.max_count,
                    match_role: None,
                    match_archetype: None,
                    match_tag: None, // already known to apply — no further filtering
                });
            }
        }
    }
    results
}

fn sample_entities(
    palette: &AtmospherePalette,
    budget: usize,
    rng: &mut impl Rng,
) -> Vec<EntityRule> {
    if palette.entities.is_empty() {
        return vec![];
    }

    let weighted: Vec<(&EntityInfluence, f64)> =
        palette.entities.iter().map(|e| (e, e.weight)).collect();

    let mut results = Vec::new();
    for _ in 0..budget {
        if let Some(influence) = weighted_sample(&weighted, rng) {
            // Only add if we haven't already added this archetype.
            if !results
                .iter()
                .any(|r: &EntityRule| r.archetype == influence.archetype)
            {
                results.push(EntityRule {
                    archetype: influence.archetype.clone(),
                    placement: influence.placement.clone(),
                    min_count: 0, // atmosphere entities are never required
                    max_count: influence.max_count,
                    behavior_tags: vec![],
                    patrol: false,
                    role_match: None,
                    archetype_match: None,
                    tag_match: None,
                    tension_min: None,
                });
            }
        }
    }
    results
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atmosphere::profile::*;
    use crate::entity::plan::EntityArchetypeId;
    use crate::entity::rules::EntityPlacementStrategy;
    use crate::feature::placement::PlacementStrategy;
    use crate::feature::registry::FeatureType;
    use crate::intent::graph::{NodeRole, ScenarioNodeId};
    use crate::spatial::plan::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn make_spec(tags: &[&str]) -> SpaceSpec {
        use crate::spatial::plan::classify_tags;
        let raw_tags: Vec<Tag> = tags.iter().map(|s| Tag::from(*s)).collect();
        let (structural, atmosphere) = classify_tags(&raw_tags);
        SpaceSpec {
            id: SpaceId(0),
            origin: ScenarioNodeId(0),
            role: NodeRole::Hub,
            structural_tags: structural,
            atmosphere_tags: atmosphere,
            motifs: vec![],
            style: RealizationStyle::RoomLike,
            kind: SpaceKind::Atomic(AtomicSpace {
                width: 7,
                height: 7,
            }),
            label: Some("test_room".into()),
            archetype: None,
            size_hint: SizeHint::Medium,
        }
    }

    fn test_profiles() -> Vec<AtmosphereProfile> {
        vec![
            AtmosphereProfile {
                name: "damp".into(),
                match_tags: vec![Tag::from("damp")],
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
                    weight: 5.0,
                }],
                entities: vec![EntityInfluence {
                    archetype: EntityArchetypeId::from("rat"),
                    placement: EntityPlacementStrategy::RandomFloor,
                    max_count: 2,
                    weight: 1.0,
                }],
            },
            AtmosphereProfile {
                name: "noble".into(),
                match_tags: vec![Tag::from("noble")],
                priority: 1,
                scatter: vec![],
                features: vec![FeatureInfluence {
                    feature_type: FeatureType::from("banner"),
                    strategy: PlacementStrategy::WallAdjacent,
                    max_count: 2,
                    weight: 3.0,
                }],
                entities: vec![],
            },
        ]
    }

    #[test]
    fn no_matching_profiles_produces_empty_contributions() {
        let profiles = test_profiles();
        let spec = make_spec(&["dry", "sealed"]);
        let mut rng = StdRng::seed_from_u64(42);

        let contrib = apply_atmosphere_influences(&profiles, &spec, 3, &mut rng);
        assert!(contrib.scatter_rules.is_empty());
        assert!(contrib.feature_rules.is_empty());
        assert!(contrib.entity_rules.is_empty());
    }

    #[test]
    fn damp_room_gets_scatter_and_features() {
        let profiles = test_profiles();
        let spec = make_spec(&["damp"]);
        let mut rng = StdRng::seed_from_u64(42);

        let contrib = apply_atmosphere_influences(&profiles, &spec, 3, &mut rng);

        // Should have water scatter.
        assert!(
            contrib
                .scatter_rules
                .iter()
                .any(|r| r.target_tile == "water")
        );
        // Should have moss features (high weight, likely sampled).
        assert!(
            contrib
                .feature_rules
                .iter()
                .any(|r| r.feature_type == FeatureType::from("moss"))
        );
        // Atmosphere features are never required.
        assert!(contrib.feature_rules.iter().all(|r| !r.required));
        // Atmosphere entities have min_count = 0.
        assert!(contrib.entity_rules.iter().all(|r| r.min_count == 0));
    }

    #[test]
    fn multiple_profiles_contribute_additively() {
        let profiles = test_profiles();
        let spec = make_spec(&["damp", "noble"]);
        let mut rng = StdRng::seed_from_u64(42);

        let contrib = apply_atmosphere_influences(&profiles, &spec, 4, &mut rng);

        // Should have features from both profiles.
        let feature_types: Vec<_> = contrib
            .feature_rules
            .iter()
            .map(|r| &r.feature_type)
            .collect();
        assert!(
            feature_types.contains(&&FeatureType::from("moss"))
                || feature_types.contains(&&FeatureType::from("banner")),
            "expected at least one feature from either damp or noble profile, got: {:?}",
            feature_types
        );
    }

    #[test]
    fn sampled_feature_rules_are_deduplicated() {
        let profiles = test_profiles();
        let spec = make_spec(&["damp"]);
        let mut rng = StdRng::seed_from_u64(42);

        // Large budget — should still not duplicate feature types.
        let contrib = apply_atmosphere_influences(&profiles, &spec, 20, &mut rng);

        let moss_count = contrib
            .feature_rules
            .iter()
            .filter(|r| r.feature_type == FeatureType::from("moss"))
            .count();
        assert!(
            moss_count <= 1,
            "moss should appear at most once, got: {}",
            moss_count
        );
    }

    #[test]
    fn scatter_rules_dedup_keeps_highest_density() {
        let profiles = vec![AtmosphereProfile {
            name: "multi_water".into(),
            match_tags: vec![Tag::from("wet")],
            priority: 0,
            scatter: vec![
                ScatterInfluence {
                    target_tile: "water".into(),
                    density: 0.10,
                    weight: 5.0,
                },
                ScatterInfluence {
                    target_tile: "water".into(),
                    density: 0.20,
                    weight: 1.0,
                },
            ],
            features: vec![],
            entities: vec![],
        }];
        let spec = make_spec(&["wet"]);
        let mut rng = StdRng::seed_from_u64(42);

        let contrib = apply_atmosphere_influences(&profiles, &spec, 10, &mut rng);

        let water_rules: Vec<_> = contrib
            .scatter_rules
            .iter()
            .filter(|r| r.target_tile == "water")
            .collect();
        assert_eq!(water_rules.len(), 1, "water scatter should be deduplicated");
        assert!(
            water_rules[0].density >= 0.10,
            "should keep a valid density"
        );
    }

    #[test]
    fn deterministic_with_same_seed() {
        let profiles = test_profiles();
        let spec = make_spec(&["damp", "noble"]);

        let mut rng1 = StdRng::seed_from_u64(12345);
        let c1 = apply_atmosphere_influences(&profiles, &spec, 4, &mut rng1);

        let mut rng2 = StdRng::seed_from_u64(12345);
        let c2 = apply_atmosphere_influences(&profiles, &spec, 4, &mut rng2);

        assert_eq!(c1.scatter_rules.len(), c2.scatter_rules.len());
        assert_eq!(c1.feature_rules.len(), c2.feature_rules.len());
        assert_eq!(c1.entity_rules.len(), c2.entity_rules.len());
    }
}
