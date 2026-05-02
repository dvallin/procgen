//! Role→feature mapping rules.
//!
//! Defines which features belong in which rooms, driven by
//! [`NodeRole`], [`SpaceArchetype`], and tags on the [`SpaceSpec`].

use crate::feature::placement::PlacementStrategy;
use crate::feature::plan::FeatureKind;
use crate::intent::graph::NodeRole;
use crate::spatial::plan::{SpaceArchetype, SpaceSpec};
use crate::tag::Tag;

/// A single rule that says "if a room matches these criteria, place this feature."
///
/// All specified criteria are ANDed — a rule with both `match_role` and
/// `match_tag` set only fires when the space satisfies *both*.
#[derive(Debug, Clone)]
pub struct FeatureRule {
    /// What feature to place.
    pub kind: FeatureKind,
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
        if let Some(role) = self.match_role {
            if spec.role != role {
                return false;
            }
        }
        if let Some(archetype) = self.match_archetype {
            if spec.archetype != Some(archetype) {
                return false;
            }
        }
        if let Some(ref tag) = self.match_tag {
            if !spec.tags.contains(tag) {
                return false;
            }
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
    vec![
        // --- Specific: archetype + tag ---
        // Workshop with a key → chest (the key room in crypt = Chapel Vestry,
        // which the planner maps Goal→Vault, but the tag drives it).
        FeatureRule {
            kind: FeatureKind::Chest,
            strategy: PlacementStrategy::Center,
            required: true,
            max_count: 1,
            match_role: None,
            match_archetype: None,
            match_tag: Some(Tag::from("contains_key")),
        },
        // --- Role-based ---
        // Hub rooms get a table and barrels.
        FeatureRule {
            kind: FeatureKind::Table,
            strategy: PlacementStrategy::Center,
            required: false,
            max_count: 1,
            match_role: Some(NodeRole::Hub),
            match_archetype: None,
            match_tag: None,
        },
        FeatureRule {
            kind: FeatureKind::Barrel,
            strategy: PlacementStrategy::WallAdjacent,
            required: false,
            max_count: 2,
            match_role: Some(NodeRole::Hub),
            match_archetype: None,
            match_tag: None,
        },
        // Reward rooms get a chest.
        FeatureRule {
            kind: FeatureKind::Chest,
            strategy: PlacementStrategy::Center,
            required: true,
            max_count: 1,
            match_role: Some(NodeRole::Reward),
            match_archetype: None,
            match_tag: None,
        },
        // Entry rooms get torches.
        FeatureRule {
            kind: FeatureKind::Decoration("torch".into()),
            strategy: PlacementStrategy::WallAdjacent,
            required: false,
            max_count: 2,
            match_role: Some(NodeRole::Entry),
            match_archetype: None,
            match_tag: None,
        },
        // --- Archetype-based ---
        // Vault / main_goal → sarcophagus + torches.
        FeatureRule {
            kind: FeatureKind::Sarcophagus,
            strategy: PlacementStrategy::Center,
            required: true,
            max_count: 1,
            match_role: None,
            match_archetype: Some(SpaceArchetype::Vault),
            match_tag: Some(Tag::from("main_goal")),
        },
        FeatureRule {
            kind: FeatureKind::Decoration("torch".into()),
            strategy: PlacementStrategy::WallAdjacent,
            required: false,
            max_count: 2,
            match_role: None,
            match_archetype: Some(SpaceArchetype::Vault),
            match_tag: Some(Tag::from("main_goal")),
        },
        // --- Tag-driven (tavern-flavored) ---
        // "barrels" tag → extra barrels.
        FeatureRule {
            kind: FeatureKind::Barrel,
            strategy: PlacementStrategy::WallAdjacent,
            required: false,
            max_count: 2,
            match_role: None,
            match_archetype: None,
            match_tag: Some(Tag::from("barrels")),
        },
        // "food" tag → shelf.
        FeatureRule {
            kind: FeatureKind::Shelf,
            strategy: PlacementStrategy::WallAdjacent,
            required: false,
            max_count: 2,
            match_role: None,
            match_archetype: None,
            match_tag: Some(Tag::from("food")),
        },
        // "smuggling" tag → hidden chest.
        FeatureRule {
            kind: FeatureKind::Chest,
            strategy: PlacementStrategy::Corner,
            required: false,
            max_count: 1,
            match_role: None,
            match_archetype: None,
            match_tag: Some(Tag::from("smuggling")),
        },
    ]
}

/// Returns only the rules that match a given space.
pub fn matching_rules<'a>(rules: &'a [FeatureRule], spec: &SpaceSpec) -> Vec<&'a FeatureRule> {
    rules.iter().filter(|r| r.matches(spec)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
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

        let kinds: Vec<&FeatureKind> = matched.iter().map(|r| &r.kind).collect();
        assert!(
            kinds.contains(&&FeatureKind::Sarcophagus),
            "vault with main_goal should get a sarcophagus, got: {:?}",
            kinds
        );
        assert!(
            kinds.contains(&&FeatureKind::Decoration("torch".into())),
            "vault with main_goal should get torches, got: {:?}",
            kinds
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

        let kinds: Vec<&FeatureKind> = matched.iter().map(|r| &r.kind).collect();
        assert!(
            kinds.contains(&&FeatureKind::Table),
            "hub should get a table, got: {:?}",
            kinds
        );
        // Should match both the Hub barrel rule AND the "barrels" tag rule.
        let barrel_count = kinds.iter().filter(|k| ***k == FeatureKind::Barrel).count();
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

        let kinds: Vec<&FeatureKind> = matched.iter().map(|r| &r.kind).collect();
        assert!(
            kinds.contains(&&FeatureKind::Shelf),
            "room with 'food' tag should get a shelf, got: {:?}",
            kinds
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
            .filter(|r| r.kind == FeatureKind::Chest)
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
            matched.iter().map(|r| &r.kind).collect::<Vec<_>>()
        );
    }
}
