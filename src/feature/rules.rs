//! Role→feature mapping rules.
//!
//! Defines which features belong in which rooms, driven by
//! [`NodeRole`], [`SpaceArchetype`], and tags on the [`SpaceSpec`].

use serde::{Deserialize, Serialize};

use crate::feature::placement::PlacementStrategy;
use crate::feature::registry::FeatureType;
use crate::intent::graph::NodeRole;
use crate::spatial::plan::{SpaceArchetype, SpaceSpec};
use crate::tag::Tag;

/// A single rule that says "if a room matches these criteria, place this feature."
///
/// All specified criteria are ANDed — a rule with both `match_role` and
/// `match_tag` set only fires when the space satisfies *both*.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureRule {
    /// What feature type to place (data-driven identifier).
    pub feature_type: FeatureType,
    /// How to position it inside the room.
    pub strategy: PlacementStrategy,
    /// If true, the planner must error when it can't place this feature.
    pub required: bool,
    /// Maximum number of instances to place per room.
    pub max_count: u32,
    /// If set, the space must have this role to match.
    pub match_role: Option<NodeRole>,
    /// If set, the space must have this archetype to match.
    pub match_archetype: Option<SpaceArchetype>,
    /// If set, the space must carry this tag to match.
    pub match_tag: Option<Tag>,
}

impl FeatureRule {
    /// Does this rule apply to the given space?
    pub fn matches(&self, spec: &SpaceSpec) -> bool {
        if let Some(role) = self.match_role
            && spec.role != role
        {
            return false;
        }
        if let Some(archetype) = self.match_archetype
            && spec.archetype != Some(archetype)
        {
            return false;
        }
        if let Some(ref tag) = self.match_tag
            && !spec.tags.contains(tag)
        {
            return false;
        }
        true
    }
}

/// Default feature rules for the MVP scenarios (crypt + tavern).
///
/// Rules are ordered so that more specific rules (archetype + tag) come
/// before broad ones (role-only or tag-only). The planner evaluates them
/// in order, respecting `max_count` per rule per room.
pub fn default_rules() -> Vec<FeatureRule> {
    crate::asset::load::load_default_feature_rules()
        .expect("embedded feature rules are valid JSON (compile-time guarantee)")
}

/// Returns only the rules that match a given space.
pub fn matching_rules<'a>(rules: &'a [FeatureRule], spec: &SpaceSpec) -> Vec<&'a FeatureRule> {
    rules.iter().filter(|r| r.matches(spec)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::plan::Feature;
    use crate::intent::graph::{NodeRole, ScenarioNodeId};
    use crate::spatial::plan::*;

    /// Helper: build a minimal SpaceSpec with the given role, archetype, and tags.
    fn make_spec(role: NodeRole, archetype: Option<SpaceArchetype>, tags: &[&str]) -> SpaceSpec {
        SpaceSpec {
            id: SpaceId(0),
            origin: ScenarioNodeId(0),
            role,
            tags: tags.iter().map(|s| Tag::from(*s)).collect(),
            style: RealizationStyle::RoomLike,
            kind: SpaceKind::Atomic(AtomicSpace {
                width: 5,
                height: 5,
            }),
            label: None,
            archetype,
            size_hint: SizeHint::Medium,
        }
    }

    #[test]
    fn crypt_vault_gets_sarcophagus_and_torches() {
        let spec = make_spec(NodeRole::Goal, Some(SpaceArchetype::Vault), &["main_goal"]);
        let rules = default_rules();
        let matched = matching_rules(&rules, &spec);

        let types: Vec<&FeatureType> = matched.iter().map(|r| &r.feature_type).collect();
        assert!(
            types.contains(&&FeatureType::from(Feature::SARCOPHAGUS)),
            "vault with main_goal should get a sarcophagus, got: {:?}",
            types
        );
        assert!(
            types.contains(&&FeatureType::from("torch")),
            "vault with main_goal should get torches, got: {:?}",
            types
        );
    }

    #[test]
    fn tavern_hub_gets_table_and_barrels() {
        let spec = make_spec(
            NodeRole::Hub,
            Some(SpaceArchetype::Hall),
            &["barrels", "damp"],
        );
        let rules = default_rules();
        let matched = matching_rules(&rules, &spec);

        let types: Vec<&FeatureType> = matched.iter().map(|r| &r.feature_type).collect();
        assert!(
            types.contains(&&FeatureType::from(Feature::TABLE)),
            "hub should get a table, got: {:?}",
            types
        );
        // Should match both the Hub barrel rule AND the "barrels" tag rule.
        let barrel_count = types
            .iter()
            .filter(|t| ***t == FeatureType::from(Feature::BARREL))
            .count();
        assert!(
            barrel_count >= 2,
            "hub with 'barrels' tag should match ≥2 barrel rules, got: {}",
            barrel_count
        );
    }

    #[test]
    fn pantry_gets_shelf_from_food_tag() {
        let spec = make_spec(
            NodeRole::Branch,
            Some(SpaceArchetype::Chamber),
            &["optional", "food"],
        );
        let rules = default_rules();
        let matched = matching_rules(&rules, &spec);

        let types: Vec<&FeatureType> = matched.iter().map(|r| &r.feature_type).collect();
        assert!(
            types.contains(&&FeatureType::from(Feature::SHELF)),
            "room with 'food' tag should get a shelf, got: {:?}",
            types
        );
    }

    #[test]
    fn key_room_gets_required_chest() {
        let spec = make_spec(
            NodeRole::Goal,
            Some(SpaceArchetype::Vault),
            &["optional", "contains_key"],
        );
        let rules = default_rules();
        let matched = matching_rules(&rules, &spec);

        let chest_rules: Vec<&&FeatureRule> = matched
            .iter()
            .filter(|r| r.feature_type == FeatureType::from(Feature::CHEST))
            .collect();
        assert!(
            !chest_rules.is_empty(),
            "room with 'contains_key' tag should get a chest"
        );
        assert!(
            chest_rules.iter().any(|r| r.required),
            "the chest in the key room should be required"
        );
    }

    #[test]
    fn plain_gate_matches_no_specific_rules() {
        let spec = make_spec(NodeRole::Gate, Some(SpaceArchetype::Vestibule), &["locked"]);
        let rules = default_rules();
        let matched = matching_rules(&rules, &spec);

        assert!(
            matched.is_empty(),
            "a plain gate/vestibule with only 'locked' tag should match no feature rules, got: {:?}",
            matched.iter().map(|r| &r.feature_type).collect::<Vec<_>>()
        );
    }

    #[test]
    fn serde_round_trip_default_rules() {
        let rules = default_rules();
        let json = serde_json::to_string_pretty(&rules).unwrap();
        let deserialized: Vec<FeatureRule> = serde_json::from_str(&json).unwrap();

        assert_eq!(rules.len(), deserialized.len());
        for (orig, deser) in rules.iter().zip(deserialized.iter()) {
            assert_eq!(orig.feature_type, deser.feature_type);
            assert_eq!(orig.strategy, deser.strategy);
            assert_eq!(orig.required, deser.required);
            assert_eq!(orig.max_count, deser.max_count);
            assert_eq!(orig.match_role, deser.match_role);
            assert_eq!(orig.match_archetype, deser.match_archetype);
            assert_eq!(orig.match_tag, deser.match_tag);
        }
    }

    #[test]
    fn serde_round_trip_from_json_file() {
        let json = include_str!("../../assets/rules/features.json");
        let rules: Vec<FeatureRule> = serde_json::from_str(json).unwrap();

        assert_eq!(rules.len(), default_rules().len());
        // Verify a known rule deserialized correctly.
        let sarcophagus_rule = rules
            .iter()
            .find(|r| r.feature_type == FeatureType::from(Feature::SARCOPHAGUS));
        assert!(sarcophagus_rule.is_some(), "should find sarcophagus rule");
        let rule = sarcophagus_rule.unwrap();
        assert!(rule.required);
        assert_eq!(rule.match_archetype, Some(SpaceArchetype::Vault));
        assert_eq!(rule.match_tag, Some(Tag::from("main_goal")));
    }

    #[test]
    fn serde_feature_type_round_trips() {
        let rule = FeatureRule {
            feature_type: FeatureType::from("banner"),
            strategy: PlacementStrategy::WallAdjacent,
            required: false,
            max_count: 3,
            match_role: Some(NodeRole::Hub),
            match_archetype: None,
            match_tag: Some(Tag::from("noble")),
        };
        let json = serde_json::to_string(&rule).unwrap();
        let deser: FeatureRule = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.feature_type, FeatureType::from("banner"));
        assert_eq!(deser.match_role, Some(NodeRole::Hub));
        assert_eq!(deser.match_tag, Some(Tag::from("noble")));
    }
}
