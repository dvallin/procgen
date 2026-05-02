//! Structural constraint inference — derives constraints from filled graph topology.
//!
//! Instead of requiring pattern authors to manually declare constraints like
//! `MustGate`, this module scans the filled [`ScenarioGraph`] and infers them
//! from structural signals:
//!
//! - **MustGate**: A node with `Gate` role that has a `RestrictedTraversal` outgoing
//!   edge implies the target must not be reachable without passing through the gate.
//! - **MaxDepth**: Honored from the pattern's `max_depth` field when present.

use crate::intent::graph::{EdgeRole, NodeRole, ScenarioGraph};
use crate::intent::map_intent::IntentConstraint;

/// Infer constraints from the topology of a filled ScenarioGraph.
///
/// This scans the graph for structural patterns that imply layout constraints:
///
/// 1. **MustGate**: For every node with `Gate` role that has an outgoing
///    `RestrictedTraversal` edge, emit a `MustGate` constraint with the gate
///    as the gate node and the edge target as the gated space.
///
/// 2. **MaxDepth**: If `max_depth` is provided (from the pattern's metadata),
///    emit a `MaxDepth` constraint.
///
/// # Arguments
///
/// * `graph` — The filled ScenarioGraph to analyze.
/// * `max_depth` — Optional max depth override from the pattern.
pub fn infer_constraints(graph: &ScenarioGraph, max_depth: Option<u32>) -> Vec<IntentConstraint> {
    let mut constraints = Vec::new();

    // Rule 1: Gate + RestrictedTraversal → MustGate
    for node in &graph.nodes {
        if node.role == NodeRole::Gate {
            // Find all RestrictedTraversal edges originating from this gate.
            for edge in &graph.edges {
                if edge.from == node.id && edge.role == EdgeRole::RestrictedTraversal {
                    // Find the target node's key.
                    if let Some(target) = graph.nodes.iter().find(|n| n.id == edge.to) {
                        constraints.push(IntentConstraint::MustGate {
                            space: target.key.clone(),
                            gate: node.key.clone(),
                        });
                    }
                }
            }
        }
    }

    // Rule 2: Pattern-level max_depth override.
    if let Some(depth) = max_depth {
        constraints.push(IntentConstraint::MaxDepth(depth));
    }

    constraints
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::load::{load_default_patterns, load_default_vocabularies};
    use crate::intent::filler::fill_pattern;
    use crate::intent::graph::*;
    use crate::intent::pattern::NarrativePattern;
    use crate::intent::vocabulary::ThemeVocabulary;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn get_pattern(id: &str) -> NarrativePattern {
        let patterns = load_default_patterns().unwrap();
        patterns.into_iter().find(|p| p.id == id).unwrap()
    }

    fn get_vocabulary(id: &str) -> ThemeVocabulary {
        let vocabs = load_default_vocabularies().unwrap();
        vocabs.into_iter().find(|v| v.id == id).unwrap()
    }

    // ─── Structural inference from filled graphs ─────────────────────────────

    #[test]
    fn lock_and_key_infers_must_gate() {
        let pattern = get_pattern("lock_and_key");
        let vocab = get_vocabulary("undead_nobility");
        let mut rng = StdRng::seed_from_u64(42);

        let filled = fill_pattern(&pattern, &vocab, &mut rng).unwrap();
        let constraints = infer_constraints(&filled.graph, pattern.max_depth);

        // Should have at least one MustGate (gate→goal is RestrictedTraversal).
        let must_gates: Vec<_> = constraints
            .iter()
            .filter(|c| matches!(c, IntentConstraint::MustGate { .. }))
            .collect();
        assert!(
            !must_gates.is_empty(),
            "lock_and_key should infer MustGate from topology"
        );

        // Specifically: gate→goal.
        let has_goal_gate = constraints.iter().any(|c| {
            matches!(c, IntentConstraint::MustGate { space, gate }
                if space == "goal" && gate == "gate")
        });
        assert!(
            has_goal_gate,
            "expected MustGate {{ space: goal, gate: gate }}, got: {:?}",
            constraints
        );
    }

    #[test]
    fn gauntlet_infers_two_must_gates() {
        let pattern = get_pattern("gauntlet");
        let vocab = get_vocabulary("undead_nobility");
        let mut rng = StdRng::seed_from_u64(42);

        let filled = fill_pattern(&pattern, &vocab, &mut rng).unwrap();
        let constraints = infer_constraints(&filled.graph, pattern.max_depth);

        let must_gates: Vec<_> = constraints
            .iter()
            .filter(|c| matches!(c, IntentConstraint::MustGate { .. }))
            .collect();
        assert_eq!(
            must_gates.len(),
            2,
            "gauntlet has 2 gates with RestrictedTraversal, got: {:?}",
            must_gates
        );

        // gate_1→challenge and gate_2→goal.
        let has_gate1 = constraints.iter().any(|c| {
            matches!(c, IntentConstraint::MustGate { space, gate }
                if space == "challenge" && gate == "gate_1")
        });
        let has_gate2 = constraints.iter().any(|c| {
            matches!(c, IntentConstraint::MustGate { space, gate }
                if space == "goal" && gate == "gate_2")
        });
        assert!(has_gate1, "expected MustGate for gate_1→challenge");
        assert!(has_gate2, "expected MustGate for gate_2→goal");
    }

    #[test]
    fn linear_descent_infers_must_gate_and_max_depth() {
        let pattern = get_pattern("linear_descent");
        let vocab = get_vocabulary("undead_nobility");
        let mut rng = StdRng::seed_from_u64(42);

        let filled = fill_pattern(&pattern, &vocab, &mut rng).unwrap();
        let constraints = infer_constraints(&filled.graph, pattern.max_depth);

        // Should have MustGate (gate→depths is RestrictedTraversal).
        let has_must_gate = constraints.iter().any(|c| {
            matches!(c, IntentConstraint::MustGate { space, gate }
                if space == "depths" && gate == "gate")
        });
        assert!(has_must_gate, "linear_descent should infer MustGate");

        // Should have MaxDepth(5) from pattern metadata.
        let has_max_depth = constraints
            .iter()
            .any(|c| matches!(c, IntentConstraint::MaxDepth(5)));
        assert!(
            has_max_depth,
            "linear_descent should emit MaxDepth(5) from pattern.max_depth"
        );
    }

    #[test]
    fn hub_and_spoke_infers_no_must_gate() {
        let pattern = get_pattern("hub_and_spoke");
        let vocab = get_vocabulary("urban_underground");
        let mut rng = StdRng::seed_from_u64(42);

        let filled = fill_pattern(&pattern, &vocab, &mut rng).unwrap();
        let constraints = infer_constraints(&filled.graph, pattern.max_depth);

        // hub_and_spoke has no Gate nodes or RestrictedTraversal, so no MustGate.
        let must_gates: Vec<_> = constraints
            .iter()
            .filter(|c| matches!(c, IntentConstraint::MustGate { .. }))
            .collect();
        assert!(
            must_gates.is_empty(),
            "hub_and_spoke should have no MustGate, got: {:?}",
            must_gates
        );

        // No max_depth either.
        assert!(constraints.is_empty());
    }

    #[test]
    fn pattern_without_max_depth_emits_none() {
        let pattern = get_pattern("lock_and_key");
        assert_eq!(pattern.max_depth, None);

        let vocab = get_vocabulary("undead_nobility");
        let mut rng = StdRng::seed_from_u64(42);
        let filled = fill_pattern(&pattern, &vocab, &mut rng).unwrap();
        let constraints = infer_constraints(&filled.graph, pattern.max_depth);

        // No MaxDepth constraint.
        let has_max_depth = constraints
            .iter()
            .any(|c| matches!(c, IntentConstraint::MaxDepth(_)));
        assert!(!has_max_depth);
    }

    // ─── Direct graph construction tests ─────────────────────────────────────

    #[test]
    fn gate_with_restricted_traversal_emits_must_gate() {
        let graph = ScenarioGraph {
            nodes: vec![
                ScenarioNode {
                    id: ScenarioNodeId(0),
                    key: "entrance".into(),
                    role: NodeRole::Entry,
                    tags: vec![],
                    label: None,
                    archetype_hint: None,
                },
                ScenarioNode {
                    id: ScenarioNodeId(1),
                    key: "my_gate".into(),
                    role: NodeRole::Gate,
                    tags: vec![],
                    label: None,
                    archetype_hint: None,
                },
                ScenarioNode {
                    id: ScenarioNodeId(2),
                    key: "treasure".into(),
                    role: NodeRole::Goal,
                    tags: vec![],
                    label: None,
                    archetype_hint: None,
                },
            ],
            edges: vec![
                ScenarioEdge {
                    from: ScenarioNodeId(0),
                    to: ScenarioNodeId(1),
                    role: EdgeRole::Traversal,
                    tags: vec![],
                },
                ScenarioEdge {
                    from: ScenarioNodeId(1),
                    to: ScenarioNodeId(2),
                    role: EdgeRole::RestrictedTraversal,
                    tags: vec![],
                },
            ],
        };

        let constraints = infer_constraints(&graph, None);

        assert_eq!(constraints.len(), 1);
        assert!(
            matches!(&constraints[0], IntentConstraint::MustGate { space, gate }
                if space == "treasure" && gate == "my_gate")
        );
    }

    #[test]
    fn gate_without_restricted_traversal_emits_nothing() {
        // A Gate node that only has Traversal edges — no MustGate inferred.
        let graph = ScenarioGraph {
            nodes: vec![
                ScenarioNode {
                    id: ScenarioNodeId(0),
                    key: "entrance".into(),
                    role: NodeRole::Entry,
                    tags: vec![],
                    label: None,
                    archetype_hint: None,
                },
                ScenarioNode {
                    id: ScenarioNodeId(1),
                    key: "gate".into(),
                    role: NodeRole::Gate,
                    tags: vec![],
                    label: None,
                    archetype_hint: None,
                },
                ScenarioNode {
                    id: ScenarioNodeId(2),
                    key: "beyond".into(),
                    role: NodeRole::Goal,
                    tags: vec![],
                    label: None,
                    archetype_hint: None,
                },
            ],
            edges: vec![
                ScenarioEdge {
                    from: ScenarioNodeId(0),
                    to: ScenarioNodeId(1),
                    role: EdgeRole::Traversal,
                    tags: vec![],
                },
                ScenarioEdge {
                    from: ScenarioNodeId(1),
                    to: ScenarioNodeId(2),
                    role: EdgeRole::Traversal, // NOT restricted!
                    tags: vec![],
                },
            ],
        };

        let constraints = infer_constraints(&graph, None);
        assert!(constraints.is_empty());
    }

    #[test]
    fn non_gate_with_restricted_traversal_emits_nothing() {
        // A Hub node with RestrictedTraversal — only Gate-role triggers MustGate.
        let graph = ScenarioGraph {
            nodes: vec![
                ScenarioNode {
                    id: ScenarioNodeId(0),
                    key: "hub".into(),
                    role: NodeRole::Hub, // NOT a gate!
                    tags: vec![],
                    label: None,
                    archetype_hint: None,
                },
                ScenarioNode {
                    id: ScenarioNodeId(1),
                    key: "secret".into(),
                    role: NodeRole::Reward,
                    tags: vec![],
                    label: None,
                    archetype_hint: None,
                },
            ],
            edges: vec![ScenarioEdge {
                from: ScenarioNodeId(0),
                to: ScenarioNodeId(1),
                role: EdgeRole::RestrictedTraversal,
                tags: vec![],
            }],
        };

        let constraints = infer_constraints(&graph, None);
        assert!(constraints.is_empty());
    }

    #[test]
    fn max_depth_override_emitted_even_with_empty_graph() {
        let graph = ScenarioGraph {
            nodes: vec![],
            edges: vec![],
        };

        let constraints = infer_constraints(&graph, Some(7));
        assert_eq!(constraints.len(), 1);
        assert!(matches!(&constraints[0], IntentConstraint::MaxDepth(7)));
    }

    #[test]
    fn multiple_gates_emit_multiple_must_gates() {
        let graph = ScenarioGraph {
            nodes: vec![
                ScenarioNode {
                    id: ScenarioNodeId(0),
                    key: "entry".into(),
                    role: NodeRole::Entry,
                    tags: vec![],
                    label: None,
                    archetype_hint: None,
                },
                ScenarioNode {
                    id: ScenarioNodeId(1),
                    key: "gate_a".into(),
                    role: NodeRole::Gate,
                    tags: vec![],
                    label: None,
                    archetype_hint: None,
                },
                ScenarioNode {
                    id: ScenarioNodeId(2),
                    key: "mid".into(),
                    role: NodeRole::Hub,
                    tags: vec![],
                    label: None,
                    archetype_hint: None,
                },
                ScenarioNode {
                    id: ScenarioNodeId(3),
                    key: "gate_b".into(),
                    role: NodeRole::Gate,
                    tags: vec![],
                    label: None,
                    archetype_hint: None,
                },
                ScenarioNode {
                    id: ScenarioNodeId(4),
                    key: "end".into(),
                    role: NodeRole::Goal,
                    tags: vec![],
                    label: None,
                    archetype_hint: None,
                },
            ],
            edges: vec![
                ScenarioEdge {
                    from: ScenarioNodeId(0),
                    to: ScenarioNodeId(1),
                    role: EdgeRole::Traversal,
                    tags: vec![],
                },
                ScenarioEdge {
                    from: ScenarioNodeId(1),
                    to: ScenarioNodeId(2),
                    role: EdgeRole::RestrictedTraversal,
                    tags: vec![],
                },
                ScenarioEdge {
                    from: ScenarioNodeId(2),
                    to: ScenarioNodeId(3),
                    role: EdgeRole::Traversal,
                    tags: vec![],
                },
                ScenarioEdge {
                    from: ScenarioNodeId(3),
                    to: ScenarioNodeId(4),
                    role: EdgeRole::RestrictedTraversal,
                    tags: vec![],
                },
            ],
        };

        let constraints = infer_constraints(&graph, None);

        assert_eq!(constraints.len(), 2);
        let has_gate_a = constraints.iter().any(|c| {
            matches!(c, IntentConstraint::MustGate { space, gate }
                if space == "mid" && gate == "gate_a")
        });
        let has_gate_b = constraints.iter().any(|c| {
            matches!(c, IntentConstraint::MustGate { space, gate }
                if space == "end" && gate == "gate_b")
        });
        assert!(has_gate_a);
        assert!(has_gate_b);
    }

    // ─── Integration: all patterns produce expected constraints ───────────────

    #[test]
    fn all_patterns_produce_consistent_constraints() {
        let patterns = load_default_patterns().unwrap();
        let vocabs = load_default_vocabularies().unwrap();

        for pattern in &patterns {
            for vocab in &vocabs {
                let mut rng = StdRng::seed_from_u64(42);
                let filled = fill_pattern(pattern, vocab, &mut rng).unwrap();
                let constraints = infer_constraints(&filled.graph, pattern.max_depth);

                // Count RestrictedTraversal edges from Gate nodes.
                let restricted_from_gates = filled
                    .graph
                    .edges
                    .iter()
                    .filter(|e| {
                        e.role == EdgeRole::RestrictedTraversal
                            && filled
                                .graph
                                .nodes
                                .iter()
                                .any(|n| n.id == e.from && n.role == NodeRole::Gate)
                    })
                    .count();

                let must_gate_count = constraints
                    .iter()
                    .filter(|c| matches!(c, IntentConstraint::MustGate { .. }))
                    .count();

                assert_eq!(
                    must_gate_count, restricted_from_gates,
                    "pattern '{}' + vocab '{}': expected {} MustGate constraints \
                     (one per Gate+RestrictedTraversal), got {}",
                    pattern.id, vocab.id, restricted_from_gates, must_gate_count
                );

                // MaxDepth should be present iff pattern.max_depth is Some.
                let has_max_depth = constraints
                    .iter()
                    .any(|c| matches!(c, IntentConstraint::MaxDepth(_)));
                assert_eq!(
                    has_max_depth,
                    pattern.max_depth.is_some(),
                    "pattern '{}': max_depth presence mismatch",
                    pattern.id
                );
            }
        }
    }
}
