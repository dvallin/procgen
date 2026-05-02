//! Slot filler — instantiates pattern slots using a theme vocabulary.
//!
//! Given a selected [`NarrativePattern`] and a [`ThemeVocabulary`], the filler
//! creates a concrete [`ScenarioGraph`] by:
//! 1. Deciding which optional slots to include (RNG-based).
//! 2. Picking a vocabulary entry for each included slot (matching by role).
//! 3. Assembling nodes and edges, skipping any edges that reference omitted slots.

use rand::Rng;

use crate::intent::graph::*;
use crate::intent::pattern::NarrativePattern;
use crate::intent::vocabulary::{ThemeVocabulary, VocabularyEntry};
use crate::tag::Tag;

/// Result of slot filling — a ScenarioGraph.
#[derive(Debug, Clone)]
pub struct FilledPattern {
    /// The instantiated scenario graph.
    pub graph: ScenarioGraph,
}

/// Errors that can occur during slot filling.
#[derive(Debug)]
pub enum SlotFillError {
    /// No vocabulary entries available for a required slot's role.
    NoEntriesForRole { slot_key: String, role: NodeRole },
}

impl std::fmt::Display for SlotFillError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoEntriesForRole { slot_key, role } => {
                write!(
                    f,
                    "no vocabulary entries for slot '{}' with role {:?}",
                    slot_key, role
                )
            }
        }
    }
}

impl std::error::Error for SlotFillError {}

/// Probability that an optional slot is included (0.0–1.0).
const OPTIONAL_SLOT_INCLUSION_PROBABILITY: f64 = 0.7;

/// Fill a pattern's slots using a theme vocabulary.
///
/// For each slot in the pattern:
/// - If `optional`, include it with probability [`OPTIONAL_SLOT_INCLUSION_PROBABILITY`].
/// - Pick a random vocabulary entry matching the slot's role.
/// - Build a `ScenarioNode` from the entry (label, tags, archetype).
///
/// Edges referencing omitted optional slots are skipped.
///
/// # Errors
///
/// Returns [`SlotFillError::NoEntriesForRole`] if a required (non-optional) slot
/// has no matching vocabulary entries.
pub fn fill_pattern<R: Rng>(
    pattern: &NarrativePattern,
    vocabulary: &ThemeVocabulary,
    rng: &mut R,
) -> Result<FilledPattern, SlotFillError> {
    // Phase 1: Decide which slots to include.
    let mut included_keys = std::collections::HashSet::new();
    let mut nodes = Vec::new();
    let mut node_index: u32 = 0;

    for slot in &pattern.slots {
        // Decide whether to include optional slots.
        if slot.optional {
            let roll: f64 = rng.r#gen();
            if roll >= OPTIONAL_SLOT_INCLUSION_PROBABILITY {
                continue; // Skip this optional slot.
            }
        }

        // Find vocabulary entries for this slot's role.
        let entries = vocabulary.entries_for_role(slot.role);

        if entries.is_empty() {
            if !slot.optional {
                return Err(SlotFillError::NoEntriesForRole {
                    slot_key: slot.key.clone(),
                    role: slot.role,
                });
            }
            // Optional slot with no entries — just skip it.
            continue;
        }

        // Pick a random entry.
        let entry_idx = rng.gen_range(0..entries.len());
        let entry: &VocabularyEntry = entries[entry_idx];

        // Build the node.
        let node = ScenarioNode {
            id: ScenarioNodeId(node_index),
            key: slot.key.clone(),
            role: slot.role,
            tags: entry.tags.iter().map(|t| Tag::from(t.as_str())).collect(),
            label: Some(entry.label.clone()),
            archetype_hint: Some(entry.archetype),
        };

        nodes.push(node);
        included_keys.insert(slot.key.clone());
        node_index += 1;
    }

    // Phase 2: Build edges, skipping any that reference omitted slots.
    let edges = build_edges(pattern, &included_keys, &nodes);

    Ok(FilledPattern {
        graph: ScenarioGraph { nodes, edges },
    })
}

/// Build edges from pattern edge templates, skipping omitted slots.
fn build_edges(
    pattern: &NarrativePattern,
    included_keys: &std::collections::HashSet<String>,
    nodes: &[ScenarioNode],
) -> Vec<ScenarioEdge> {
    let mut edges = Vec::new();

    for pattern_edge in &pattern.edges {
        // Skip edges referencing omitted slots.
        if !included_keys.contains(&pattern_edge.from) || !included_keys.contains(&pattern_edge.to)
        {
            continue;
        }

        // Look up node IDs by key.
        let from_id = nodes
            .iter()
            .find(|n| n.key == pattern_edge.from)
            .map(|n| n.id);
        let to_id = nodes
            .iter()
            .find(|n| n.key == pattern_edge.to)
            .map(|n| n.id);

        if let (Some(from), Some(to)) = (from_id, to_id) {
            edges.push(ScenarioEdge {
                from,
                to,
                role: pattern_edge.role,
                tags: vec![],
            });
        }
    }

    edges
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::load::{load_default_patterns, load_default_vocabularies};
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

    #[test]
    fn fill_lock_and_key_with_undead_produces_valid_graph() {
        let pattern = get_pattern("lock_and_key");
        let vocab = get_vocabulary("undead_nobility");
        let mut rng = StdRng::seed_from_u64(42);

        let result = fill_pattern(&pattern, &vocab, &mut rng).unwrap();
        let graph = &result.graph;

        // Must have at least the required slots: entry, hub, gate, goal (4 required).
        assert!(graph.nodes.len() >= 4);
        // At most all 6 slots (including 2 optional).
        assert!(graph.nodes.len() <= 6);

        // All nodes should have labels from the vocabulary.
        for node in &graph.nodes {
            assert!(node.label.is_some());
            assert!(node.archetype_hint.is_some());
        }

        // Must have an entry node.
        assert!(graph.nodes.iter().any(|n| n.role == NodeRole::Entry));
        // Must have a hub node.
        assert!(graph.nodes.iter().any(|n| n.role == NodeRole::Hub));
        // Must have a gate node.
        assert!(graph.nodes.iter().any(|n| n.role == NodeRole::Gate));
    }

    #[test]
    fn fill_hub_and_spoke_with_urban_produces_valid_graph() {
        let pattern = get_pattern("hub_and_spoke");
        let vocab = get_vocabulary("urban_underground");
        let mut rng = StdRng::seed_from_u64(123);

        let result = fill_pattern(&pattern, &vocab, &mut rng).unwrap();
        let graph = &result.graph;

        // Required slots: entry, hub, spoke_1 (3 required).
        assert!(graph.nodes.len() >= 3);
        assert!(graph.nodes.len() <= 6);

        // Entry and Hub must be present.
        assert!(graph.nodes.iter().any(|n| n.role == NodeRole::Entry));
        assert!(graph.nodes.iter().any(|n| n.role == NodeRole::Hub));
    }

    #[test]
    fn filled_graph_edges_reference_existing_nodes() {
        let pattern = get_pattern("lock_and_key");
        let vocab = get_vocabulary("undead_nobility");
        let mut rng = StdRng::seed_from_u64(42);

        let result = fill_pattern(&pattern, &vocab, &mut rng).unwrap();
        let graph = &result.graph;

        let node_ids: Vec<ScenarioNodeId> = graph.nodes.iter().map(|n| n.id).collect();
        for edge in &graph.edges {
            assert!(
                node_ids.contains(&edge.from),
                "edge from {:?} references missing node",
                edge.from
            );
            assert!(
                node_ids.contains(&edge.to),
                "edge to {:?} references missing node",
                edge.to
            );
        }
    }

    #[test]
    fn different_seeds_produce_different_labels() {
        let pattern = get_pattern("lock_and_key");
        let vocab = get_vocabulary("undead_nobility");

        let mut labels_seen = std::collections::HashSet::new();
        for seed in 0..50 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = fill_pattern(&pattern, &vocab, &mut rng).unwrap();
            let hub = result.graph.nodes.iter().find(|n| n.key == "hub").unwrap();
            labels_seen.insert(hub.label.clone().unwrap());
        }
        // undead_nobility has 2 Hub entries, so we should see variety.
        assert!(
            labels_seen.len() >= 2,
            "expected label variety across seeds, got: {:?}",
            labels_seen
        );
    }

    #[test]
    fn optional_slots_sometimes_included_sometimes_omitted() {
        let pattern = get_pattern("lock_and_key");
        let vocab = get_vocabulary("undead_nobility");

        let mut counts = Vec::new();
        for seed in 0..100 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = fill_pattern(&pattern, &vocab, &mut rng).unwrap();
            counts.push(result.graph.nodes.len());
        }

        let min = *counts.iter().min().unwrap();
        let max = *counts.iter().max().unwrap();
        // With 70% inclusion probability over 100 runs, we should see variation.
        assert!(
            min < max,
            "expected optional slot variation, but all runs had {} nodes",
            min
        );
    }

    #[test]
    fn fill_is_deterministic_with_same_seed() {
        let pattern = get_pattern("lock_and_key");
        let vocab = get_vocabulary("undead_nobility");

        let mut rng1 = StdRng::seed_from_u64(77);
        let result1 = fill_pattern(&pattern, &vocab, &mut rng1).unwrap();

        let mut rng2 = StdRng::seed_from_u64(77);
        let result2 = fill_pattern(&pattern, &vocab, &mut rng2).unwrap();

        assert_eq!(result1.graph.nodes.len(), result2.graph.nodes.len());
        for (n1, n2) in result1.graph.nodes.iter().zip(result2.graph.nodes.iter()) {
            assert_eq!(n1.key, n2.key);
            assert_eq!(n1.label, n2.label);
            assert_eq!(n1.role, n2.role);
        }
    }

    #[test]
    fn all_patterns_fillable_with_all_vocabularies() {
        let patterns = load_default_patterns().unwrap();
        let vocabs = load_default_vocabularies().unwrap();

        for pattern in &patterns {
            for vocab in &vocabs {
                let mut rng = StdRng::seed_from_u64(42);
                let result = fill_pattern(pattern, vocab, &mut rng);
                assert!(
                    result.is_ok(),
                    "pattern '{}' + vocab '{}' failed: {:?}",
                    pattern.id,
                    vocab.id,
                    result.err()
                );
            }
        }
    }
}
