//! Role→entity mapping rules.
//!
//! Defines which entities belong in which rooms, driven by
//! [`NodeRole`], [`SpaceArchetype`], and tags on the [`SpaceSpec`].

use serde::{Deserialize, Serialize};

use crate::entity::plan::EntityArchetypeId;
use crate::feature::registry::FeatureType;
use crate::intent::graph::NodeRole;
use crate::spatial::plan::{SpaceArchetype, SpaceSpec};
use crate::tag::Tag;
use crate::tension::TensionLevel;

/// How an entity should be positioned inside a room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityPlacementStrategy {
    /// Place at (or nearest to) the geometric center of the room.
    Center,
    /// Place on any available floor tile (furthest from occupied).
    RandomFloor,
    /// Place near the room's door(s) — prefer tiles adjacent to doors.
    NearEntrance,
    /// Place near a specific feature type (e.g. mimic near a chest).
    NearFeature(FeatureType),
}

/// A single rule that says "if a room matches these criteria, spawn this entity."
///
/// All specified criteria are ANDed — a rule with both `role_match` and
/// `tag_match` set only fires when the space satisfies *both*.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRule {
    /// What entity archetype to spawn.
    pub archetype: EntityArchetypeId,
    /// How to position it inside the room.
    pub placement: EntityPlacementStrategy,
    /// Minimum instances to place per room. If > 0 and placement fails, it's an error.
    pub min_count: u32,
    /// Maximum instances to place per room.
    pub max_count: u32,
    /// Behavior tags applied to spawned entities (e.g. "patrols", "stationary").
    pub behavior_tags: Vec<Tag>,
    /// Whether the spawned entity should have a patrol zone computed.
    pub patrol: bool,
    /// If set, the space must have this role to match.
    pub role_match: Option<NodeRole>,
    /// If set, the space must have this archetype to match.
    pub archetype_match: Option<SpaceArchetype>,
    /// If set, the space must carry this tag to match.
    pub tag_match: Option<Tag>,
    /// If set, the room must have at least this tension level for the rule to fire.
    /// Rules without this field always match (regardless of tension).
    #[serde(default)]
    pub tension_min: Option<TensionLevel>,
}

impl EntityRule {
    /// Does this rule apply to the given space?
    pub fn matches(&self, spec: &SpaceSpec) -> bool {
        if let Some(role) = self.role_match
            && spec.role != role
        {
            return false;
        }
        if let Some(archetype) = self.archetype_match
            && spec.archetype != Some(archetype)
        {
            return false;
        }
        if let Some(ref tag) = self.tag_match
            && !spec.has_tag(tag)
        {
            return false;
        }
        true
    }

    /// Does this rule's tension requirement allow it to fire at the given level?
    /// Returns `true` if `tension_min` is `None` or the room's tension is ≥ the minimum.
    pub fn tension_allows(&self, room_tension: TensionLevel) -> bool {
        match self.tension_min {
            None => true,
            Some(min) => room_tension >= min,
        }
    }
}

/// Default entity rules for the MVP scenarios (crypt + tavern).
///
/// Rules are ordered so that more specific rules (role + tag) come before
/// broad ones (tag-only). The planner evaluates them in order, respecting
/// `max_count` per rule per room.
pub fn default_entity_rules() -> Vec<EntityRule> {
    crate::asset::load::load_default_entity_rules()
        .expect("embedded entity rules are valid JSON (compile-time guarantee)")
}

/// Returns only the rules that match a given space.
pub fn matching_entity_rules<'a>(
    rules: &'a [EntityRule],
    spec: &SpaceSpec,
    tension: Option<TensionLevel>,
) -> Vec<&'a EntityRule> {
    rules
        .iter()
        .filter(|r| r.matches(spec))
        .filter(|r| match tension {
            Some(t) => r.tension_allows(t),
            None => true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::graph::ScenarioNodeId;
    use crate::spatial::plan::*;

    /// Helper: build a minimal SpaceSpec with the given role, archetype, and tags.
    fn make_spec(role: NodeRole, archetype: Option<SpaceArchetype>, tags: &[&str]) -> SpaceSpec {
        use crate::spatial::plan::classify_tags;
        let raw_tags: Vec<Tag> = tags.iter().map(|s| Tag::from(*s)).collect();
        let (structural, atmosphere) = classify_tags(&raw_tags);
        SpaceSpec {
            id: SpaceId(0),
            origin: ScenarioNodeId(0),
            role,
            structural_tags: structural,
            atmosphere_tags: atmosphere,
            motifs: vec![],
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
    fn goal_main_goal_gets_skeleton_guardians() {
        let spec = make_spec(NodeRole::Goal, Some(SpaceArchetype::Vault), &["main_goal"]);
        let rules = default_entity_rules();
        let matched = matching_entity_rules(&rules, &spec, None);

        let archetypes: Vec<&EntityArchetypeId> = matched.iter().map(|r| &r.archetype).collect();
        assert!(
            archetypes.contains(&&EntityArchetypeId::from("skeleton_guardian")),
            "goal with main_goal should get skeleton_guardian, got: {:?}",
            archetypes
        );
        // Verify it's required (min_count > 0).
        let guardian_rule = matched
            .iter()
            .find(|r| r.archetype == EntityArchetypeId::from("skeleton_guardian"))
            .unwrap();
        assert!(
            guardian_rule.min_count > 0,
            "skeleton_guardian should be required"
        );
        assert!(guardian_rule.patrol, "skeleton_guardian should patrol");
    }

    #[test]
    fn hub_gets_no_structural_entities() {
        let spec = make_spec(
            NodeRole::Hub,
            Some(SpaceArchetype::Hall),
            &["noble", "sealed"],
        );
        let rules = default_entity_rules();
        let matched = matching_entity_rules(&rules, &spec, None);

        assert!(
            matched.is_empty(),
            "hub rooms should have no structural entity rules (atmosphere handles ambient entities), got: {:?}",
            matched.iter().map(|r| &r.archetype).collect::<Vec<_>>()
        );
    }

    #[test]
    fn hub_with_barrels_gets_no_structural_entities() {
        let spec = make_spec(
            NodeRole::Hub,
            Some(SpaceArchetype::Hall),
            &["barrels", "damp"],
        );
        let rules = default_entity_rules();
        let matched = matching_entity_rules(&rules, &spec, None);

        assert!(
            matched.is_empty(),
            "hub rooms with barrels should have no structural entity rules (atmosphere handles rats), got: {:?}",
            matched.iter().map(|r| &r.archetype).collect::<Vec<_>>()
        );
    }

    #[test]
    fn entry_gets_no_entities() {
        let spec = make_spec(
            NodeRole::Entry,
            Some(SpaceArchetype::Vestibule),
            &["noble", "sealed"],
        );
        let rules = default_entity_rules();
        let matched = matching_entity_rules(&rules, &spec, None);

        assert!(
            matched.is_empty(),
            "entry rooms should have no entity rules, got: {:?}",
            matched.iter().map(|r| &r.archetype).collect::<Vec<_>>()
        );
    }

    #[test]
    fn smuggling_tag_gets_smuggler() {
        let spec = make_spec(
            NodeRole::Reward,
            Some(SpaceArchetype::Chamber),
            &["secret", "smuggling"],
        );
        let rules = default_entity_rules();
        let matched = matching_entity_rules(&rules, &spec, None);

        let archetypes: Vec<&EntityArchetypeId> = matched.iter().map(|r| &r.archetype).collect();
        assert!(
            archetypes.contains(&&EntityArchetypeId::from("smuggler")),
            "room with 'smuggling' tag should get a smuggler, got: {:?}",
            archetypes
        );
        let smuggler_rule = matched
            .iter()
            .find(|r| r.archetype == EntityArchetypeId::from("smuggler"))
            .unwrap();
        assert_eq!(smuggler_rule.min_count, 1, "smuggler should be required");
        assert!(!smuggler_rule.patrol, "smuggler should be stationary");
    }

    #[test]
    fn gate_without_locked_tag_gets_no_entities() {
        let spec = make_spec(NodeRole::Gate, Some(SpaceArchetype::Vestibule), &["hidden"]);
        let rules = default_entity_rules();
        let matched = matching_entity_rules(&rules, &spec, None);

        assert!(
            matched.is_empty(),
            "gate without 'locked' tag should match no entity rules, got: {:?}",
            matched.iter().map(|r| &r.archetype).collect::<Vec<_>>()
        );
    }

    #[test]
    fn serde_round_trip_default_entity_rules() {
        let rules = default_entity_rules();
        let json = serde_json::to_string_pretty(&rules).unwrap();
        let deserialized: Vec<EntityRule> = serde_json::from_str(&json).unwrap();

        assert_eq!(rules.len(), deserialized.len());
        for (orig, deser) in rules.iter().zip(deserialized.iter()) {
            assert_eq!(orig.archetype, deser.archetype);
            assert_eq!(orig.placement, deser.placement);
            assert_eq!(orig.min_count, deser.min_count);
            assert_eq!(orig.max_count, deser.max_count);
            assert_eq!(orig.behavior_tags, deser.behavior_tags);
            assert_eq!(orig.patrol, deser.patrol);
            assert_eq!(orig.role_match, deser.role_match);
            assert_eq!(orig.archetype_match, deser.archetype_match);
            assert_eq!(orig.tag_match, deser.tag_match);
        }
    }

    #[test]
    fn serde_round_trip_from_json_file() {
        let json = include_str!("../../assets/rules/entities.json");
        let rules: Vec<EntityRule> = serde_json::from_str(json).unwrap();

        assert_eq!(rules.len(), default_entity_rules().len());
        // Verify a known rule deserialized correctly.
        let guardian_rule = rules
            .iter()
            .find(|r| r.archetype == EntityArchetypeId::from("skeleton_guardian"));
        assert!(
            guardian_rule.is_some(),
            "should find skeleton_guardian rule"
        );
        let rule = guardian_rule.unwrap();
        assert_eq!(rule.min_count, 1);
        assert_eq!(rule.max_count, 2);
        assert!(rule.patrol);
        assert_eq!(rule.role_match, Some(NodeRole::Goal));
        assert_eq!(rule.tag_match, Some(Tag::from("main_goal")));
    }

    #[test]
    fn serde_near_feature_variant_round_trips() {
        let rule = EntityRule {
            archetype: EntityArchetypeId::from("trap_mimic"),
            placement: EntityPlacementStrategy::NearFeature(FeatureType::from("trap")),
            min_count: 0,
            max_count: 1,
            behavior_tags: vec![Tag::from("stationary")],
            patrol: false,
            role_match: None,
            archetype_match: Some(SpaceArchetype::Chamber),
            tag_match: Some(Tag::from("trapped")),
            tension_min: None,
        };
        let json = serde_json::to_string(&rule).unwrap();
        let deser: EntityRule = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deser.placement,
            EntityPlacementStrategy::NearFeature(FeatureType::from("trap"))
        );
        assert_eq!(deser.archetype_match, Some(SpaceArchetype::Chamber));
        assert_eq!(deser.tag_match, Some(Tag::from("trapped")));
    }

    #[test]
    fn tension_min_filters_low_tension_rooms() {
        let rule = EntityRule {
            archetype: EntityArchetypeId::from("skeleton"),
            placement: EntityPlacementStrategy::RandomFloor,
            min_count: 0,
            max_count: 2,
            behavior_tags: vec![],
            patrol: false,
            role_match: None,
            archetype_match: None,
            tag_match: None,
            tension_min: Some(TensionLevel::Medium),
        };

        // Low tension — should NOT match
        assert!(!rule.tension_allows(TensionLevel::Low));
        // Medium tension — should match (equal to min)
        assert!(rule.tension_allows(TensionLevel::Medium));
        // High tension — should match (above min)
        assert!(rule.tension_allows(TensionLevel::High));
    }

    #[test]
    fn tension_min_none_always_matches() {
        let rule = EntityRule {
            archetype: EntityArchetypeId::from("rat"),
            placement: EntityPlacementStrategy::RandomFloor,
            min_count: 0,
            max_count: 1,
            behavior_tags: vec![],
            patrol: false,
            role_match: None,
            archetype_match: None,
            tag_match: None,
            tension_min: None,
        };

        assert!(rule.tension_allows(TensionLevel::Low));
        assert!(rule.tension_allows(TensionLevel::Climax));
    }

    #[test]
    fn matching_entity_rules_filters_by_tension() {
        let rules = vec![
            EntityRule {
                archetype: EntityArchetypeId::from("rat"),
                placement: EntityPlacementStrategy::RandomFloor,
                min_count: 0,
                max_count: 1,
                behavior_tags: vec![],
                patrol: false,
                role_match: None,
                archetype_match: None,
                tag_match: None,
                tension_min: None, // always matches
            },
            EntityRule {
                archetype: EntityArchetypeId::from("skeleton"),
                placement: EntityPlacementStrategy::RandomFloor,
                min_count: 0,
                max_count: 2,
                behavior_tags: vec![],
                patrol: false,
                role_match: None,
                archetype_match: None,
                tag_match: None,
                tension_min: Some(TensionLevel::High), // only High+
            },
        ];

        let spec = make_spec(NodeRole::Hub, Some(SpaceArchetype::Hall), &[]);

        // At Low tension — only rat matches
        let matched = matching_entity_rules(&rules, &spec, Some(TensionLevel::Low));
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].archetype, EntityArchetypeId::from("rat"));

        // At High tension — both match
        let matched = matching_entity_rules(&rules, &spec, Some(TensionLevel::High));
        assert_eq!(matched.len(), 2);
    }

    #[test]
    fn serde_tension_min_deserializes() {
        let json = r#"{
            "archetype": "skeleton",
            "placement": "RandomFloor",
            "min_count": 0,
            "max_count": 2,
            "behavior_tags": [],
            "patrol": false,
            "role_match": null,
            "archetype_match": null,
            "tag_match": null,
            "tension_min": "High"
        }"#;
        let rule: EntityRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.tension_min, Some(TensionLevel::High));
    }

    #[test]
    fn serde_tension_min_absent_deserializes_to_none() {
        let json = r#"{
            "archetype": "rat",
            "placement": "RandomFloor",
            "min_count": 0,
            "max_count": 1,
            "behavior_tags": [],
            "patrol": false,
            "role_match": null,
            "archetype_match": null,
            "tag_match": null
        }"#;
        let rule: EntityRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.tension_min, None);
    }
}
