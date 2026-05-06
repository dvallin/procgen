//! Interior templates — declarative zone→feature layouts for rooms.
//!
//! An `InteriorTemplate` specifies which features should appear in which zones
//! of a room. Templates match rooms by role, archetype, and/or tags, providing
//! a constraint-based approach to room furnishing.
//!
//! Example: a crypt vault template might say "altar at center, torches on wall-band,
//! keep door-path clear." The feature planner uses these directives for zone-aware
//! placement, falling back to generic rules for unmatched rooms.

use serde::{Deserialize, Serialize};

use crate::feature::registry::FeatureType;
use crate::intent::graph::NodeRole;
use crate::spatial::plan::{SpaceArchetype, SpaceSpec};
use crate::tag::Tag;

use super::plan::ZoneKind;

// ─── InteriorTemplate ───────────────────────────────────────────────────────

/// A declarative interior layout template.
///
/// When a room matches the template's criteria, the feature planner
/// uses its directives for zone-aware placement instead of (or in
/// addition to) generic feature rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteriorTemplate {
    /// Human-readable name for debugging.
    pub name: String,
    /// If set, the room must have this role.
    pub match_role: Option<NodeRole>,
    /// If set, the room must have this archetype.
    pub match_archetype: Option<SpaceArchetype>,
    /// If set, the room must carry this tag (checked across all tag fields).
    pub match_tag: Option<Tag>,
    /// Zone placement directives.
    pub directives: Vec<ZoneDirective>,
    /// Priority for tie-breaking when multiple templates match.
    /// Higher priority wins.
    #[serde(default)]
    pub priority: i32,
}

impl InteriorTemplate {
    /// Check if this template matches a room's spec.
    ///
    /// All specified criteria are ANDed — a template with both `match_role`
    /// and `match_tag` set only matches when the space satisfies *both*.
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
            if !spec.has_tag(tag) {
                return false;
            }
        }
        true
    }
}

// ─── ZoneDirective ──────────────────────────────────────────────────────────

/// A single zone→placement directive within a template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneDirective {
    /// Which zone this directive targets.
    pub zone: ZoneKind,
    /// What to do with this zone.
    pub action: ZoneAction,
}

// ─── ZoneAction ─────────────────────────────────────────────────────────────

/// What action to take in a zone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ZoneAction {
    /// Place a specific feature type in this zone.
    Place {
        feature_type: FeatureType,
        max_count: u32,
    },
    /// Keep this zone clear of blocking features.
    Clear,
}

// ─── Matching ───────────────────────────────────────────────────────────────

/// Find the best matching template for a room, preferring higher priority.
/// Returns `None` if no template matches.
pub fn match_template<'a>(
    templates: &'a [InteriorTemplate],
    spec: &SpaceSpec,
) -> Option<&'a InteriorTemplate> {
    templates
        .iter()
        .filter(|t| t.matches(spec))
        .max_by_key(|t| t.priority)
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::graph::ScenarioNodeId;
    use crate::spatial::plan::*;

    /// Helper: build a minimal SpaceSpec with the given role, archetype, and tags.
    fn make_spec(role: NodeRole, archetype: Option<SpaceArchetype>, tags: &[&str]) -> SpaceSpec {
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
            max_connectors: None,
            connector_distribution: None,
        }
    }

    /// Helper: build a crypt_vault template for tests.
    fn crypt_vault_template() -> InteriorTemplate {
        InteriorTemplate {
            name: "crypt_vault".into(),
            match_role: Some(NodeRole::Goal),
            match_archetype: Some(SpaceArchetype::Vault),
            match_tag: Some(Tag::from("main_goal")),
            directives: vec![
                ZoneDirective {
                    zone: ZoneKind::Center,
                    action: ZoneAction::Place {
                        feature_type: FeatureType::from("sarcophagus"),
                        max_count: 1,
                    },
                },
                ZoneDirective {
                    zone: ZoneKind::WallBand,
                    action: ZoneAction::Place {
                        feature_type: FeatureType::from("torch"),
                        max_count: 2,
                    },
                },
                ZoneDirective {
                    zone: ZoneKind::DoorPath,
                    action: ZoneAction::Clear,
                },
            ],
            priority: 10,
        }
    }

    #[test]
    fn vault_goal_room_matches_crypt_vault_template() {
        let template = crypt_vault_template();
        let spec = make_spec(NodeRole::Goal, Some(SpaceArchetype::Vault), &["main_goal"]);
        assert!(
            template.matches(&spec),
            "a Goal/Vault room with main_goal tag should match the crypt_vault template"
        );
    }

    #[test]
    fn hub_room_does_not_match_vault_template() {
        let template = crypt_vault_template();
        let spec = make_spec(NodeRole::Hub, Some(SpaceArchetype::Hall), &["noble"]);
        assert!(
            !template.matches(&spec),
            "a Hub/Hall room should not match the crypt_vault template"
        );
    }

    #[test]
    fn match_template_returns_highest_priority() {
        let low = InteriorTemplate {
            name: "treasure_room".into(),
            match_role: Some(NodeRole::Reward),
            match_archetype: None,
            match_tag: None,
            directives: vec![ZoneDirective {
                zone: ZoneKind::Center,
                action: ZoneAction::Place {
                    feature_type: FeatureType::from("chest"),
                    max_count: 1,
                },
            }],
            priority: 3,
        };
        let high = InteriorTemplate {
            name: "special_reward".into(),
            match_role: Some(NodeRole::Reward),
            match_archetype: None,
            match_tag: None,
            directives: vec![ZoneDirective {
                zone: ZoneKind::Center,
                action: ZoneAction::Place {
                    feature_type: FeatureType::from("altar"),
                    max_count: 1,
                },
            }],
            priority: 8,
        };
        let unrelated = crypt_vault_template(); // priority 10, but won't match Reward

        let templates = vec![low, high, unrelated];
        let spec = make_spec(NodeRole::Reward, None, &[]);

        let matched = match_template(&templates, &spec);
        assert!(matched.is_some(), "should find a matching template");
        assert_eq!(
            matched.unwrap().name,
            "special_reward",
            "should pick the highest-priority match"
        );
    }

    #[test]
    fn match_template_returns_none_when_nothing_matches() {
        let templates = vec![crypt_vault_template()];
        let spec = make_spec(NodeRole::Entry, Some(SpaceArchetype::Vestibule), &[]);

        assert!(
            match_template(&templates, &spec).is_none(),
            "should return None when no template matches"
        );
    }

    #[test]
    fn serde_round_trip() {
        let template = crypt_vault_template();
        let json = serde_json::to_string_pretty(&template).unwrap();
        let deserialized: InteriorTemplate = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, template.name);
        assert_eq!(deserialized.match_role, template.match_role);
        assert_eq!(deserialized.match_archetype, template.match_archetype);
        assert_eq!(deserialized.match_tag, template.match_tag);
        assert_eq!(deserialized.priority, template.priority);
        assert_eq!(deserialized.directives.len(), template.directives.len());
    }

    #[test]
    fn serde_round_trip_from_json_asset() {
        let json = include_str!("../../assets/rules/interior_templates.json");
        let templates: Vec<InteriorTemplate> =
            serde_json::from_str(json).expect("interior_templates.json should parse");

        assert_eq!(templates.len(), 6, "should have 6 starter templates");

        let crypt = templates.iter().find(|t| t.name == "crypt_vault");
        assert!(crypt.is_some(), "should contain crypt_vault template");
        let crypt = crypt.unwrap();
        assert_eq!(crypt.match_role, Some(NodeRole::Goal));
        assert_eq!(crypt.match_archetype, Some(SpaceArchetype::Vault));
        assert_eq!(crypt.match_tag, Some(Tag::from("main_goal")));
        assert_eq!(crypt.priority, 10);
        assert_eq!(crypt.directives.len(), 3);

        // Verify re-serialization round-trips cleanly.
        let reserialized = serde_json::to_string_pretty(&templates).unwrap();
        let re_parsed: Vec<InteriorTemplate> = serde_json::from_str(&reserialized).unwrap();
        assert_eq!(re_parsed.len(), templates.len());
    }
}
