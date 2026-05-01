use crate::intent::builder::{IntentBuildError, IntentBuilder};
use crate::intent::graph::*;
use crate::intent::map_intent::{IntentConstraint, LocationKind, MapIntent, MapScale, MotifId};
use crate::situation::SituationContext;
use crate::tag::Tag;

/// Creates the narrative context for the tavern cellar scenario.
pub fn build_tavern_situation() -> SituationContext {
    SituationContext::new(vec![
        Tag::from("tavern"),
        Tag::from("cellar"),
        Tag::from("smuggling"),
        Tag::from("dock_district"),
    ])
    .with_binding("location", "tavern_cellar")
    .with_binding("theme", "urban_underground")
}

/// Builds a MapIntent for the tavern cellar scenario from a SituationContext.
pub struct TavernIntentBuilder;

impl IntentBuilder for TavernIntentBuilder {
    fn build(&self, situation: &SituationContext) -> Result<MapIntent, IntentBuildError> {
        if !situation.bindings.contains_key("location") {
            return Err(IntentBuildError::MissingRequiredBinding(
                "location".to_string(),
            ));
        }

        let structural_graph = build_tavern_graph();

        Ok(MapIntent {
            location_kind: LocationKind::Building,
            scale: MapScale::Small,
            tags: situation.tags.clone(),
            motifs: vec![MotifId::from("timber"), MotifId::from("damp")],
            structural_graph,
            constraints: vec![
                IntentConstraint::MustConnect {
                    from: "taproom".to_string(),
                    to: "pantry".to_string(),
                },
                IntentConstraint::MustGate {
                    space: "tunnel".to_string(),
                    gate: "cold_room".to_string(),
                },
            ],
        })
    }
}

/// Build the tavern cellar structural graph.
///
/// Layout concept:
///   Staircase → Taproom → Wine Store
///                  ├─→ Pantry
///                  └─→ Cold Room →* Secret Tunnel
///
/// Different from the crypt: uses Building-style archetypes,
/// has a branch structure rather than a linear critical path.
fn build_tavern_graph() -> ScenarioGraph {
    let staircase = ScenarioNodeId(0);
    let taproom = ScenarioNodeId(1);
    let wine_store = ScenarioNodeId(2);
    let pantry = ScenarioNodeId(3);
    let cold_room = ScenarioNodeId(4);
    let tunnel = ScenarioNodeId(5);

    ScenarioGraph {
        nodes: vec![
            ScenarioNode {
                id: staircase,
                key: "staircase".into(),
                role: NodeRole::Entry,
                tags: vec![Tag::from("narrow"), Tag::from("wooden")],
                label: Some("Cellar Stairs".into()),
            },
            ScenarioNode {
                id: taproom,
                key: "taproom".into(),
                role: NodeRole::Hub,
                tags: vec![Tag::from("barrels"), Tag::from("damp")],
                label: Some("Taproom Cellar".into()),
            },
            ScenarioNode {
                id: wine_store,
                key: "wine_store".into(),
                role: NodeRole::Goal,
                tags: vec![Tag::from("main_goal"), Tag::from("valuable")],
                label: Some("Wine Store".into()),
            },
            ScenarioNode {
                id: pantry,
                key: "pantry".into(),
                role: NodeRole::Branch,
                tags: vec![Tag::from("optional"), Tag::from("food")],
                label: Some("Pantry".into()),
            },
            ScenarioNode {
                id: cold_room,
                key: "cold_room".into(),
                role: NodeRole::Gate,
                tags: vec![Tag::from("hidden")],
                label: Some("Cold Room".into()),
            },
            ScenarioNode {
                id: tunnel,
                key: "tunnel".into(),
                role: NodeRole::Reward,
                tags: vec![Tag::from("secret"), Tag::from("smuggling")],
                label: Some("Smuggler's Tunnel".into()),
            },
        ],
        edges: vec![
            ScenarioEdge {
                from: staircase,
                to: taproom,
                role: EdgeRole::Traversal,
                tags: vec![],
            },
            ScenarioEdge {
                from: taproom,
                to: wine_store,
                role: EdgeRole::Traversal,
                tags: vec![],
            },
            ScenarioEdge {
                from: taproom,
                to: pantry,
                role: EdgeRole::OptionalTraversal,
                tags: vec![],
            },
            ScenarioEdge {
                from: taproom,
                to: cold_room,
                role: EdgeRole::SecretTraversal,
                tags: vec![Tag::from("hidden_door")],
            },
            ScenarioEdge {
                from: cold_room,
                to: tunnel,
                role: EdgeRole::RestrictedTraversal,
                tags: vec![],
            },
        ],
    }
}
