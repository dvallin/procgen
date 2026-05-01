use crate::intent::builder::{IntentBuildError, IntentBuilder};
use crate::intent::graph::*;
use crate::intent::map_intent::{IntentConstraint, LocationKind, MapIntent, MapScale, MotifId};
use crate::situation::SituationContext;
use crate::tag::Tag;

// --- Phase 2: SituationContext + IntentBuilder path ---

/// Creates the narrative context for the noble crypt scenario.
pub fn build_crypt_situation() -> SituationContext {
    SituationContext::new(vec![
        Tag::from("noble_family"),
        Tag::from("sealed_crypt"),
        Tag::from("burial_ground"),
        Tag::from("locked_vault"),
    ])
    .with_binding("location", "crypt")
    .with_binding("theme", "undead_nobility")
}

/// Builds a MapIntent for the noble crypt scenario from a SituationContext.
pub struct CryptIntentBuilder;

impl IntentBuilder for CryptIntentBuilder {
    fn build(&self, situation: &SituationContext) -> Result<MapIntent, IntentBuildError> {
        // Validate that we have the expected location binding.
        if !situation.bindings.contains_key("location") {
            return Err(IntentBuildError::MissingRequiredBinding(
                "location".to_string(),
            ));
        }

        let structural_graph = build_crypt_graph();

        Ok(MapIntent {
            location_kind: LocationKind::Dungeon,
            scale: MapScale::Small,
            tags: situation.tags.clone(),
            motifs: vec![MotifId::from("gothic"), MotifId::from("stone")],
            structural_graph,
            constraints: vec![IntentConstraint::MustGate {
                space: "goal".to_string(),
                gate: "gate".to_string(),
            }],
        })
    }
}

// --- Legacy: direct ScenarioGraph construction (used by integration tests) ---

/// Legacy entry point — builds the crypt ScenarioGraph directly.
/// Prefer `CryptIntentBuilder` + `build_crypt_situation()` for new code.
pub fn build_demo_scenario() -> ScenarioGraph {
    build_crypt_graph()
}

/// Internal helper shared by both the legacy path and `CryptIntentBuilder`.
fn build_crypt_graph() -> ScenarioGraph {
    let entrance = ScenarioNodeId(0);
    let hub = ScenarioNodeId(1);
    let key_area = ScenarioNodeId(2);
    let gate = ScenarioNodeId(3);
    let goal = ScenarioNodeId(4);
    let reward = ScenarioNodeId(5);

    ScenarioGraph {
        nodes: vec![
            ScenarioNode {
                id: entrance,
                key: "entrance".into(),
                role: NodeRole::Entry,
                tags: vec![Tag::from("noble"), Tag::from("sealed")],
                label: Some("Breach".into()),
            },
            ScenarioNode {
                id: hub,
                key: "hub".into(),
                role: NodeRole::Hub,
                tags: vec![Tag::from("noble"), Tag::from("sealed")],
                label: Some("Great Hall".into()),
            },
            ScenarioNode {
                id: key_area,
                key: "key_area".into(),
                role: NodeRole::Goal,
                tags: vec![Tag::from("optional"), Tag::from("contains_key")],
                label: Some("Chapel Vestry".into()),
            },
            ScenarioNode {
                id: gate,
                key: "gate".into(),
                role: NodeRole::Gate,
                tags: vec![Tag::from("locked")],
                label: Some("Sealed Crypt Door".into()),
            },
            ScenarioNode {
                id: goal,
                key: "goal".into(),
                role: NodeRole::Goal,
                tags: vec![Tag::from("main_goal")],
                label: Some("Family Vault".into()),
            },
            ScenarioNode {
                id: reward,
                key: "reward".into(),
                role: NodeRole::Reward,
                tags: vec![Tag::from("optional")],
                label: Some("Reliquary".into()),
            },
        ],
        edges: vec![
            ScenarioEdge {
                from: entrance,
                to: hub,
                role: EdgeRole::Traversal,
                tags: vec![],
            },
            ScenarioEdge {
                from: hub,
                to: gate,
                role: EdgeRole::Traversal,
                tags: vec![],
            },
            ScenarioEdge {
                from: gate,
                to: goal,
                role: EdgeRole::RestrictedTraversal,
                tags: vec![],
            },
            ScenarioEdge {
                from: hub,
                to: key_area,
                role: EdgeRole::OptionalTraversal,
                tags: vec![],
            },
            ScenarioEdge {
                from: hub,
                to: reward,
                role: EdgeRole::OptionalTraversal,
                tags: vec![],
            },
        ],
    }
}
