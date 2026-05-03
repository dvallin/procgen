use std::collections::HashMap;

use tracing::{debug, info, info_span};

use crate::intent::graph::*;
use crate::intent::map_intent::MapIntent;
use crate::spatial::plan::classify_tags;
use crate::spatial::plan::*;
use crate::tag::Tag;

#[derive(Debug)]
pub enum SpatialPlanError {
    MissingSpaceForScenarioNode(ScenarioNodeId),
}

impl std::fmt::Display for SpatialPlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSpaceForScenarioNode(id) => {
                write!(f, "missing space for scenario node {:?}", id)
            }
        }
    }
}

impl std::error::Error for SpatialPlanError {}

/// Converts a MapIntent into a SpatialPlan.
/// The intent provides the structural graph plus metadata (scale, constraints)
/// that the planner can use to make sizing and topology decisions.
pub trait SpatialPlanner {
    fn plan(&self, intent: &MapIntent) -> Result<SpatialPlan, SpatialPlanError>;
}

#[derive(Default)]
pub struct SimpleSpatialPlanner;

impl SpatialPlanner for SimpleSpatialPlanner {
    fn plan(&self, intent: &MapIntent) -> Result<SpatialPlan, SpatialPlanError> {
        let _span = info_span!("spatial_planning").entered();
        let scenario = &intent.structural_graph;
        let mut mapping = HashMap::<ScenarioNodeId, SpaceId>::new();
        let mut spaces = Vec::new();

        for (index, node) in scenario.nodes.iter().enumerate() {
            let id = SpaceId(index as u32);
            mapping.insert(node.id, id);

            // Derive archetype and size hint from the node's role.
            let archetype = node
                .archetype_hint
                .or_else(|| default_archetype_for_role(node.role));
            let size_hint = default_size_hint_for_role(node.role);

            // Resolve concrete dimensions from size_hint + archetype.
            let (width, height) = resolve_dimensions(size_hint, archetype);

            let (structural, atmosphere) = classify_tags(&node.tags);

            spaces.push(SpaceSpec {
                id,
                origin: node.id,
                role: node.role,
                structural_tags: structural,
                atmosphere_tags: atmosphere,
                motifs: vec![],
                style: RealizationStyle::RoomLike,
                kind: SpaceKind::Atomic(AtomicSpace { width, height }),
                label: node.label.clone(),
                archetype,
                size_hint,
            });
        }

        for space in &spaces {
            let (w, h) = match &space.kind {
                SpaceKind::Atomic(a) => (a.width, a.height),
            };
            debug!(
                id = space.id.0,
                role = ?space.role,
                archetype = ?space.archetype,
                width = w,
                height = h,
                label = ?space.label,
                "resolved space"
            );
        }

        let mut links = Vec::new();
        for edge in &scenario.edges {
            let from = *mapping
                .get(&edge.from)
                .ok_or(SpatialPlanError::MissingSpaceForScenarioNode(edge.from))?;
            let to = *mapping
                .get(&edge.to)
                .ok_or(SpatialPlanError::MissingSpaceForScenarioNode(edge.to))?;

            links.push(SpaceLink {
                from,
                to,
                role: edge.role,
                tags: edge.tags.iter().map(|t| Tag::from(t.0.as_str())).collect(),
            });
        }

        for link in &links {
            debug!(
                from = link.from.0,
                to = link.to.0,
                role = ?link.role,
                "created link"
            );
        }

        // Derive constraints from the structural graph.
        // For now, just mark gate relationships.
        let constraints = derive_default_constraints(&spaces, scenario);

        for constraint in &constraints {
            debug!(?constraint, "derived constraint");
        }

        info!(
            spaces = spaces.len(),
            links = links.len(),
            constraints = constraints.len(),
            "spatial plan complete"
        );

        Ok(SpatialPlan {
            spaces,
            links,
            constraints,
            location_kind: intent.location_kind,
        })
    }
}

/// Map a node role to a default archetype.
/// Returns `None` for roles that don't have an obvious archetype.
fn default_archetype_for_role(role: NodeRole) -> Option<SpaceArchetype> {
    match role {
        NodeRole::Hub => Some(SpaceArchetype::Hall),
        NodeRole::Entry => Some(SpaceArchetype::Vestibule),
        NodeRole::Gate => Some(SpaceArchetype::Vestibule),
        NodeRole::Goal => Some(SpaceArchetype::Vault),
        NodeRole::Reward => Some(SpaceArchetype::Chamber),
        NodeRole::Branch => Some(SpaceArchetype::Chamber),
        NodeRole::Transition => Some(SpaceArchetype::Corridor),
    }
}

/// Map a node role to a default size hint.
fn default_size_hint_for_role(role: NodeRole) -> SizeHint {
    match role {
        NodeRole::Hub => SizeHint::Large,
        NodeRole::Gate => SizeHint::Small,
        NodeRole::Goal => SizeHint::Medium,
        NodeRole::Reward => SizeHint::Medium,
        NodeRole::Entry => SizeHint::Medium,
        NodeRole::Branch => SizeHint::Small,
        NodeRole::Transition => SizeHint::Tiny,
    }
}

/// Resolve a SizeHint + optional SpaceArchetype into concrete (width, height).
///
/// The size hint provides a base dimension range. The archetype adjusts
/// proportions within that range:
/// - **Chamber**: prefers square proportions (min(w,h) × min(w,h))
/// - **Corridor**: prefers elongated proportions (wider or taller)
/// - **Hall/Vault/Vestibule/etc**: uses the base rectangular proportions
fn resolve_dimensions(size_hint: SizeHint, archetype: Option<SpaceArchetype>) -> (i32, i32) {
    // Base dimensions: the "standard" size for each hint tier.
    let (base_w, base_h) = match size_hint {
        SizeHint::Tiny => (5, 5),
        SizeHint::Small => (7, 5),
        SizeHint::Medium => (9, 7),
        SizeHint::Large => (11, 9),
        SizeHint::Grand => (15, 13),
        SizeHint::Custom {
            min_w,
            min_h,
            max_w,
            max_h,
        } => (
            // Use midpoint for deterministic default
            (min_w + max_w) / 2,
            (min_h + max_h) / 2,
        ),
    };

    // Archetype-driven proportion adjustment.
    match archetype {
        Some(SpaceArchetype::Chamber) => {
            // Chambers are squarish — use the smaller dimension for both axes.
            let side = base_w.min(base_h);
            (side, side)
        }
        Some(SpaceArchetype::Corridor) => {
            // Corridors are elongated — stretch the wider axis further.
            if base_w >= base_h {
                (base_w + 2, base_h.max(3))
            } else {
                (base_w.max(3), base_h + 2)
            }
        }
        // All other archetypes (Hall, Vault, Vestibule, Shaft, Courtyard, Workshop)
        // use the base rectangular proportions directly.
        _ => (base_w, base_h),
    }
}

/// Derive spatial constraints from the structural graph.
/// Currently infers `GatedBy` from restricted traversal edges.
fn derive_default_constraints(
    spaces: &[SpaceSpec],
    graph: &ScenarioGraph,
) -> Vec<SpatialConstraint> {
    let mut constraints = Vec::new();

    // Infer GatedBy constraints: if a gate node has a restricted edge
    // leading to a goal, the goal is gated by that node.
    for edge in &graph.edges {
        if edge.role == EdgeRole::RestrictedTraversal {
            // Find the spaces corresponding to from/to
            let gate_space = spaces.iter().find(|s| s.origin == edge.from);
            let target_space = spaces.iter().find(|s| s.origin == edge.to);

            if let (Some(gate), Some(target)) = (gate_space, target_space) {
                constraints.push(SpatialConstraint::GatedBy {
                    space: target.id,
                    gate: gate.id,
                });
            }
        }
    }

    // Hub spaces should prefer central placement
    for space in spaces {
        if space.role == NodeRole::Hub {
            constraints.push(SpatialConstraint::PreferCentral { space: space.id });
        }
        if space.role == NodeRole::Entry {
            constraints.push(SpatialConstraint::PreferPerimeter { space: space.id });
        }
    }

    constraints
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::map_intent::{LocationKind, MapScale};
    use crate::tag::Tag;
    use proptest::prelude::*;

    // --- Unit tests for resolve_dimensions ---

    #[test]
    fn resolve_dimensions_hall_large() {
        // Hub → Large + Hall → (11, 9)
        let (w, h) = resolve_dimensions(SizeHint::Large, Some(SpaceArchetype::Hall));
        assert_eq!((w, h), (11, 9));
    }

    #[test]
    fn resolve_dimensions_vestibule_small() {
        // Gate → Small + Vestibule → (7, 5)
        let (w, h) = resolve_dimensions(SizeHint::Small, Some(SpaceArchetype::Vestibule));
        assert_eq!((w, h), (7, 5));
    }

    #[test]
    fn resolve_dimensions_vault_medium() {
        // Goal → Medium + Vault → (9, 7)
        let (w, h) = resolve_dimensions(SizeHint::Medium, Some(SpaceArchetype::Vault));
        assert_eq!((w, h), (9, 7));
    }

    #[test]
    fn resolve_dimensions_chamber_medium() {
        // Reward → Medium + Chamber → (7, 7) — squarified
        let (w, h) = resolve_dimensions(SizeHint::Medium, Some(SpaceArchetype::Chamber));
        assert_eq!((w, h), (7, 7));
    }

    #[test]
    fn resolve_dimensions_corridor_tiny() {
        // Corridor + Tiny → elongated: (5+2, 5) = (7, 5)
        let (w, h) = resolve_dimensions(SizeHint::Tiny, Some(SpaceArchetype::Corridor));
        assert_eq!((w, h), (7, 5));
    }

    #[test]
    fn resolve_dimensions_none_archetype() {
        // No archetype → base dimensions unchanged
        let (w, h) = resolve_dimensions(SizeHint::Medium, None);
        assert_eq!((w, h), (9, 7));
    }

    #[test]
    fn resolve_dimensions_custom() {
        let (w, h) = resolve_dimensions(
            SizeHint::Custom {
                min_w: 6,
                min_h: 4,
                max_w: 10,
                max_h: 8,
            },
            None,
        );
        assert_eq!((w, h), (8, 6)); // midpoints
    }

    /// Wraps a ScenarioGraph in a minimal MapIntent for testing.
    fn wrap_in_intent(graph: ScenarioGraph) -> MapIntent {
        MapIntent {
            location_kind: LocationKind::Dungeon,
            scale: MapScale::Small,
            tags: vec![],
            motifs: vec![],
            structural_graph: graph,
            constraints: vec![],
        }
    }

    fn node_role_strategy() -> impl Strategy<Value = NodeRole> {
        prop_oneof![
            Just(NodeRole::Entry),
            Just(NodeRole::Hub),
            Just(NodeRole::Gate),
            Just(NodeRole::Goal),
            Just(NodeRole::Reward),
            Just(NodeRole::Branch),
            Just(NodeRole::Transition),
        ]
    }

    fn edge_role_strategy() -> impl Strategy<Value = EdgeRole> {
        prop_oneof![
            Just(EdgeRole::Traversal),
            Just(EdgeRole::OptionalTraversal),
            Just(EdgeRole::RestrictedTraversal),
            Just(EdgeRole::SecretTraversal),
            Just(EdgeRole::VerticalTraversal),
        ]
    }

    fn tag_strategy() -> impl Strategy<Value = Tag> {
        "[a-z]{3,8}".prop_map(|s| Tag::from(s.as_str()))
    }

    prop_compose! {
        fn arb_scenario_graph()(node_count in 1usize..=10)
            (nodes in prop::collection::vec(
                (node_role_strategy(), prop::collection::vec(tag_strategy(), 0..=3)),
                node_count..=node_count,
            ),
            edges in prop::collection::vec(
                (0u32..(node_count as u32), 0u32..(node_count as u32), edge_role_strategy()),
                0..=15,
            )) -> ScenarioGraph
        {
            let scenario_nodes: Vec<ScenarioNode> = nodes
                .into_iter()
                .enumerate()
                .map(|(i, (role, tags))| ScenarioNode {
                    id: ScenarioNodeId(i as u32),
                    key: format!("node_{}", i),
                    role,
                    tags,
                    label: Some(format!("Node {}", i)),
                    archetype_hint: None,
                })
                .collect();

            let scenario_edges: Vec<ScenarioEdge> = edges
                .into_iter()
                .map(|(from, to, role)| ScenarioEdge {
                    from: ScenarioNodeId(from),
                    to: ScenarioNodeId(to),
                    role,
                    tags: vec![],
                })
                .collect();

            ScenarioGraph {
                nodes: scenario_nodes,
                edges: scenario_edges,
            }
        }
    }

    proptest! {
        #[test]
        fn structure_preservation(graph in arb_scenario_graph()) {
            let planner = SimpleSpatialPlanner;
            let node_count = graph.nodes.len();
            let edge_count = graph.edges.len();
            let intent = wrap_in_intent(graph);
            let plan = planner.plan(&intent).unwrap();
            prop_assert_eq!(plan.spaces.len(), node_count);
            prop_assert_eq!(plan.links.len(), edge_count);
        }

        #[test]
        fn role_preservation(graph in arb_scenario_graph()) {
            let planner = SimpleSpatialPlanner;
            let nodes: Vec<_> = graph.nodes.iter().map(|n| n.role).collect();
            let intent = wrap_in_intent(graph);
            let plan = planner.plan(&intent).unwrap();
            for (space, role) in plan.spaces.iter().zip(nodes.iter()) {
                prop_assert_eq!(space.role, *role);
            }
        }

        #[test]
        fn tag_preservation(graph in arb_scenario_graph()) {
            let planner = SimpleSpatialPlanner;
            let node_tags: Vec<_> = graph.nodes.iter().map(|n| n.tags.clone()).collect();
            let intent = wrap_in_intent(graph);
            let plan = planner.plan(&intent).unwrap();
            for (space, original_tags) in plan.spaces.iter().zip(node_tags.iter()) {
                // All original tags should be preserved across the classified fields.
                let all_tags: Vec<Tag> = space.structural_tags.iter()
                    .chain(space.atmosphere_tags.iter())
                    .chain(space.motifs.iter())
                    .cloned()
                    .collect();
                prop_assert_eq!(all_tags.len(), original_tags.len());
                for tag in original_tags {
                    prop_assert!(space.has_tag(tag), "tag {:?} should be preserved", tag);
                }
            }
        }

        #[test]
        fn link_validity(graph in arb_scenario_graph()) {
            let planner = SimpleSpatialPlanner;
            let intent = wrap_in_intent(graph);
            let plan = planner.plan(&intent).unwrap();
            let valid_ids: std::collections::HashSet<SpaceId> =
                plan.spaces.iter().map(|s| s.id).collect();
            for link in &plan.links {
                prop_assert!(valid_ids.contains(&link.from),
                    "link.from {:?} not in valid space ids", link.from);
                prop_assert!(valid_ids.contains(&link.to),
                    "link.to {:?} not in valid space ids", link.to);
            }
        }

        #[test]
        fn positive_dimensions(graph in arb_scenario_graph()) {
            let planner = SimpleSpatialPlanner;
            let intent = wrap_in_intent(graph);
            let plan = planner.plan(&intent).unwrap();
            for space in &plan.spaces {
                match &space.kind {
                    SpaceKind::Atomic(atomic) => {
                        prop_assert!(atomic.width > 0,
                            "space {:?} has non-positive width {}", space.id, atomic.width);
                        prop_assert!(atomic.height > 0,
                            "space {:?} has non-positive height {}", space.id, atomic.height);
                    }
                }
            }
        }

        #[test]
        fn all_spaces_have_archetype(graph in arb_scenario_graph()) {
            let planner = SimpleSpatialPlanner;
            let intent = wrap_in_intent(graph);
            let plan = planner.plan(&intent).unwrap();
            for space in &plan.spaces {
                prop_assert!(space.archetype.is_some(),
                    "space {:?} (role {:?}) should have a default archetype",
                    space.id, space.role);
            }
        }

        #[test]
        fn constraints_reference_valid_spaces(graph in arb_scenario_graph()) {
            let planner = SimpleSpatialPlanner;
            let intent = wrap_in_intent(graph);
            let plan = planner.plan(&intent).unwrap();
            let valid_ids: std::collections::HashSet<SpaceId> =
                plan.spaces.iter().map(|s| s.id).collect();
            for constraint in &plan.constraints {
                match constraint {
                    SpatialConstraint::MustBeAdjacent { a, b } => {
                        prop_assert!(valid_ids.contains(a));
                        prop_assert!(valid_ids.contains(b));
                    }
                    SpatialConstraint::MustBeSeparated { a, b } => {
                        prop_assert!(valid_ids.contains(a));
                        prop_assert!(valid_ids.contains(b));
                    }
                    SpatialConstraint::MaxDistance { a, b, .. } => {
                        prop_assert!(valid_ids.contains(a));
                        prop_assert!(valid_ids.contains(b));
                    }
                    SpatialConstraint::GatedBy { space, gate } => {
                        prop_assert!(valid_ids.contains(space));
                        prop_assert!(valid_ids.contains(gate));
                    }
                    SpatialConstraint::PreferPerimeter { space } => {
                        prop_assert!(valid_ids.contains(space));
                    }
                    SpatialConstraint::PreferCentral { space } => {
                        prop_assert!(valid_ids.contains(space));
                    }
                }
            }
        }
    }
}
