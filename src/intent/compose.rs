//! Pattern composition — recursive sub-pattern expansion for expandable slots.
//!
//! When a [`NarrativePattern`] contains expandable slots, the composition system
//! can replace those slots with entire sub-patterns chosen via the same voting
//! mechanism used for top-level pattern selection. This produces richer, more
//! varied scenario graphs without requiring hand-authored mega-patterns.
//!
//! The main entry point is [`fill_pattern_composed`], which works like the
//! standard [`crate::intent::filler::fill_pattern`] but recursively expands
//! eligible slots into sub-graphs, respecting an expansion budget to prevent
//! unbounded growth.

use rand::Rng;
use std::collections::{HashMap, HashSet};

use crate::intent::filler::{FilledPattern, SlotFillError};
use crate::intent::graph::*;
use crate::intent::pattern::NarrativePattern;
use crate::intent::selector::score_pattern;
use crate::intent::vocabulary::ThemeVocabulary;
use crate::situation::SituationContext;
use crate::tag::Tag;

/// Probability that an expandable slot is expanded (when budget allows).
/// Slightly lower than optional inclusion (0.7) since expansion has more
/// structural impact on the resulting graph.
const EXPANSION_PROBABILITY: f64 = 0.6;

/// Probability that an optional slot is included (mirrors filler.rs).
const OPTIONAL_SLOT_INCLUSION_PROBABILITY: f64 = 0.7;

/// Maximum number of required slots a candidate sub-pattern may have.
/// Prevents explosion from overly complex sub-patterns.
const MAX_CANDIDATE_REQUIRED_SLOTS: usize = 8;

/// Context for recursive pattern composition.
///
/// Holds all shared state needed during the recursive fill process.
/// References are borrowed from the caller to avoid cloning large data structures.
pub struct CompositionContext<'a> {
    /// The full pattern library available for sub-pattern selection.
    pub patterns: &'a [NarrativePattern],
    /// Theme vocabulary for filling slots with concrete entries.
    pub vocabulary: &'a ThemeVocabulary,
    /// Situation context providing tags for sub-pattern voting.
    pub situation: &'a SituationContext,
    /// Remaining expansion budget — decremented on each recursive expansion.
    /// When 0, no further expansions occur (slots are filled normally).
    pub expansion_budget: u32,
    /// Pattern IDs already used in the current recursion stack.
    /// Prevents a pattern from expanding into itself (directly or transitively).
    pub excluded_pattern_ids: Vec<String>,
}

/// Fill a pattern's slots with recursive sub-pattern expansion.
///
/// For each slot in the pattern:
/// - Optional slots are included with probability [`OPTIONAL_SLOT_INCLUSION_PROBABILITY`].
/// - Expandable slots (when budget > 0) are expanded with probability [`EXPANSION_PROBABILITY`].
/// - When expansion occurs, a sub-pattern is selected via voting and recursively filled.
/// - The sub-pattern's graph is grafted into the parent graph, with the sub-pattern's
///   Entry node replacing the expanded slot.
///
/// # Errors
///
/// Returns [`SlotFillError::NoEntriesForRole`] if a required (non-optional, non-expanded)
/// slot has no matching vocabulary entries.
pub fn fill_pattern_composed<R: Rng>(
    pattern: &NarrativePattern,
    ctx: &CompositionContext,
    rng: &mut R,
) -> Result<FilledPattern, SlotFillError> {
    let mut nodes: Vec<ScenarioNode> = Vec::new();
    let mut included_keys: HashSet<String> = HashSet::new();
    let mut expanded_slots: Vec<(String, ScenarioGraph)> = Vec::new();
    let mut next_id: u32 = 0;
    let mut budget_remaining = ctx.expansion_budget;

    for slot in &pattern.slots {
        // Decide whether to include optional slots.
        if slot.optional {
            let roll: f64 = rng.r#gen();
            if roll >= OPTIONAL_SLOT_INCLUSION_PROBABILITY {
                continue; // Skip this optional slot.
            }
        }

        // Decide: expand or fill normally?
        let should_expand =
            slot.expandable && budget_remaining > 0 && rng.r#gen::<f64>() < EXPANSION_PROBABILITY;

        if should_expand {
            // Try to select a sub-pattern for this slot.
            if let Some(sub_pattern) = select_expansion_pattern(
                ctx.patterns,
                ctx.situation,
                slot.role,
                &ctx.excluded_pattern_ids,
                budget_remaining,
                rng,
            ) {
                // Build a child context with decremented budget and the sub-pattern excluded.
                let mut child_excluded = ctx.excluded_pattern_ids.clone();
                child_excluded.push(sub_pattern.id.clone());

                let sub_ctx = CompositionContext {
                    patterns: ctx.patterns,
                    vocabulary: ctx.vocabulary,
                    situation: ctx.situation,
                    expansion_budget: budget_remaining - 1,
                    excluded_pattern_ids: child_excluded,
                };

                let sub_filled = fill_pattern_composed(sub_pattern, &sub_ctx, rng)?;

                included_keys.insert(slot.key.clone());
                expanded_slots.push((slot.key.clone(), sub_filled.graph));
                budget_remaining -= 1;
                continue;
            }
        }

        // Normal fill (not expanding or expansion failed).
        let entries = ctx.vocabulary.entries_for_role(slot.role);

        if entries.is_empty() {
            if !slot.optional {
                return Err(SlotFillError::NoEntriesForRole {
                    slot_key: slot.key.clone(),
                    role: slot.role,
                });
            }
            // Optional slot with no entries — skip.
            continue;
        }

        // Pick a random entry.
        let entry_idx = rng.gen_range(0..entries.len());
        let entry = entries[entry_idx];

        let node = ScenarioNode {
            id: ScenarioNodeId(next_id),
            key: slot.key.clone(),
            role: slot.role,
            tags: entry.tags.iter().map(|t| Tag::from(t.as_str())).collect(),
            label: Some(entry.label.clone()),
            archetype_hint: Some(entry.archetype),
        };

        nodes.push(node);
        included_keys.insert(slot.key.clone());
        next_id += 1;
    }

    // Graft expanded sub-graphs into the parent node/edge lists.
    let mut sub_edges: Vec<ScenarioEdge> = Vec::new();
    for (slot_key, sub_graph) in expanded_slots {
        let grafted_edges = graft_subgraph(&mut nodes, &slot_key, sub_graph, &mut next_id);
        sub_edges.extend(grafted_edges);
    }

    // Build parent-level edges (skipping edges that reference omitted slots).
    let mut edges = build_composed_edges(pattern, &included_keys, &nodes);

    // Append sub-graph internal edges.
    edges.extend(sub_edges);

    Ok(FilledPattern {
        graph: ScenarioGraph { nodes, edges },
    })
}

/// Select a sub-pattern to expand an expandable slot.
///
/// Filters the pattern library to exclude already-used patterns and patterns
/// that are too large, then scores remaining candidates using situation tags
/// plus expansion vote bonuses for the parent slot's role.
///
/// Returns `None` if no eligible candidates remain.
fn select_expansion_pattern<'a, R: Rng>(
    patterns: &'a [NarrativePattern],
    situation: &SituationContext,
    parent_slot_role: NodeRole,
    excluded_ids: &[String],
    _remaining_budget: u32,
    rng: &mut R,
) -> Option<&'a NarrativePattern> {
    // Filter candidates.
    let candidates: Vec<&NarrativePattern> = patterns
        .iter()
        .filter(|p| {
            // Exclude patterns already in the recursion stack.
            if excluded_ids.contains(&p.id) {
                return false;
            }
            // Exclude patterns with too many required slots.
            let required_count = p
                .slots
                .iter()
                .filter(|s| !s.optional && !s.expandable)
                .count();
            if required_count > MAX_CANDIDATE_REQUIRED_SLOTS {
                return false;
            }
            true
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }

    // Score each candidate: base tag score + expansion vote bonus.
    let mut scored: Vec<(&NarrativePattern, i32)> = candidates
        .into_iter()
        .map(|p| {
            let base_score = score_pattern(p, &situation.tags);
            let expansion_bonus: i32 = p
                .expansion_votes
                .iter()
                .filter(|ev| ev.parent_role == parent_slot_role)
                .map(|ev| ev.weight)
                .sum();
            (p, base_score + expansion_bonus)
        })
        .collect();

    // Sort descending by score.
    scored.sort_by(|a, b| b.1.cmp(&a.1));

    let best_score = scored[0].1;

    // Collect all tied for best score.
    let tied: Vec<&NarrativePattern> = scored
        .iter()
        .take_while(|(_, s)| *s == best_score)
        .map(|(p, _)| *p)
        .collect();

    // Pick one uniformly at random among tied candidates.
    let pick = rng.gen_range(0..tied.len());
    Some(tied[pick])
}

/// Graft a sub-pattern's filled graph into the parent node list.
///
/// - The sub-graph's Entry node is given a namespaced key (`"{slot_key}.{original_key}"`)
///   and a fresh ID from the parent's `next_id` counter.
/// - All other sub-graph nodes are similarly namespaced and re-IDed.
/// - Sub-graph edges are remapped to use the new IDs.
///
/// Returns the remapped sub-graph edges (to be appended to the parent's edge list).
fn graft_subgraph(
    parent_nodes: &mut Vec<ScenarioNode>,
    slot_key: &str,
    sub_graph: ScenarioGraph,
    next_id: &mut u32,
) -> Vec<ScenarioEdge> {
    let mut id_map: HashMap<ScenarioNodeId, ScenarioNodeId> = HashMap::new();

    for mut node in sub_graph.nodes {
        let new_id = ScenarioNodeId(*next_id);
        id_map.insert(node.id, new_id);
        node.id = new_id;
        node.key = format!("{}.{}", slot_key, node.key);
        *next_id += 1;
        parent_nodes.push(node);
    }

    let mut remapped_edges = Vec::new();
    for mut edge in sub_graph.edges {
        edge.from = id_map[&edge.from];
        edge.to = id_map[&edge.to];
        remapped_edges.push(edge);
    }

    remapped_edges
}

/// Build parent-level edges from the pattern's edge templates.
///
/// For expanded slots, the Entry node of the sub-graph serves as the
/// connection point (found via the key `"{slot_key}.entry"`). For normal
/// slots, the node is found by its original key.
///
/// Edges referencing omitted (excluded) slots are skipped.
fn build_composed_edges(
    pattern: &NarrativePattern,
    included_keys: &HashSet<String>,
    nodes: &[ScenarioNode],
) -> Vec<ScenarioEdge> {
    let mut edges = Vec::new();

    for pattern_edge in &pattern.edges {
        // Skip edges referencing omitted slots.
        if !included_keys.contains(&pattern_edge.from) || !included_keys.contains(&pattern_edge.to)
        {
            continue;
        }

        let from_id = find_node_id(nodes, &pattern_edge.from);
        let to_id = find_node_id(nodes, &pattern_edge.to);

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

/// Find a node's ID by slot key.
///
/// Tries an exact key match first (for non-expanded slots), then looks for
/// the expanded sub-graph's entry point at `"{slot_key}.entry"`.
/// Also tries `"{slot_key}.{slot_key}"` as a fallback for patterns whose
/// Entry slot has a non-"entry" key matching the pattern key convention.
fn find_node_id(nodes: &[ScenarioNode], slot_key: &str) -> Option<ScenarioNodeId> {
    // Exact match (non-expanded slot).
    if let Some(node) = nodes.iter().find(|n| n.key == slot_key) {
        return Some(node.id);
    }

    // Expanded sub-graph: look for the Entry node prefixed with the slot key.
    // Convention: the Entry slot in most patterns has key "entry".
    let expanded_key = format!("{}.entry", slot_key);
    if let Some(node) = nodes.iter().find(|n| n.key == expanded_key) {
        return Some(node.id);
    }

    // Fallback: find ANY node with the slot_key prefix that has Entry role.
    let prefix = format!("{}.", slot_key);
    nodes
        .iter()
        .find(|n| n.key.starts_with(&prefix) && n.role == NodeRole::Entry)
        .map(|n| n.id)
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::load::{load_default_patterns, load_default_vocabularies};
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    /// Helper: build a SituationContext from a list of tag strings.
    fn make_situation(tags: &[&str]) -> SituationContext {
        SituationContext::new(tags.iter().map(|t| Tag::from(*t)).collect())
    }

    /// Helper: build a minimal pattern with expandable slots for testing.
    fn make_expandable_pattern() -> NarrativePattern {
        use crate::intent::pattern::{PatternEdge, PatternSlot, PatternVote};

        NarrativePattern {
            id: "test_parent".to_string(),
            name: "Test Parent".to_string(),
            description: "A test pattern with an expandable branch".to_string(),
            slots: vec![
                PatternSlot {
                    key: "entry".to_string(),
                    role: NodeRole::Entry,
                    optional: false,
                    expandable: false,
                },
                PatternSlot {
                    key: "hub".to_string(),
                    role: NodeRole::Hub,
                    optional: false,
                    expandable: false,
                },
                PatternSlot {
                    key: "branch".to_string(),
                    role: NodeRole::Branch,
                    optional: false,
                    expandable: true,
                },
            ],
            edges: vec![
                PatternEdge {
                    from: "entry".to_string(),
                    to: "hub".to_string(),
                    role: EdgeRole::Traversal,
                },
                PatternEdge {
                    from: "hub".to_string(),
                    to: "branch".to_string(),
                    role: EdgeRole::Traversal,
                },
            ],
            max_depth: None,
            votes: vec![PatternVote {
                tag: "test".to_string(),
                weight: 5,
            }],
            expansion_votes: vec![],
        }
    }

    /// Helper: build a small "child" pattern that can fill a Branch slot.
    fn make_child_pattern() -> NarrativePattern {
        use crate::intent::pattern::{ExpansionVote, PatternEdge, PatternSlot, PatternVote};

        NarrativePattern {
            id: "test_child".to_string(),
            name: "Test Child".to_string(),
            description: "A simple child pattern".to_string(),
            slots: vec![
                PatternSlot {
                    key: "entry".to_string(),
                    role: NodeRole::Entry,
                    optional: false,
                    expandable: false,
                },
                PatternSlot {
                    key: "goal".to_string(),
                    role: NodeRole::Goal,
                    optional: false,
                    expandable: false,
                },
            ],
            edges: vec![PatternEdge {
                from: "entry".to_string(),
                to: "goal".to_string(),
                role: EdgeRole::Traversal,
            }],
            max_depth: None,
            votes: vec![PatternVote {
                tag: "test".to_string(),
                weight: 3,
            }],
            expansion_votes: vec![ExpansionVote {
                parent_role: NodeRole::Branch,
                weight: 5,
            }],
        }
    }

    /// Helper: get the default vocabulary for undead_nobility.
    fn get_vocabulary() -> ThemeVocabulary {
        let vocabs = load_default_vocabularies().unwrap();
        vocabs
            .into_iter()
            .find(|v| v.id == "undead_nobility")
            .unwrap()
    }

    #[test]
    fn test_graft_subgraph_namespaces_keys() {
        let mut parent_nodes = vec![ScenarioNode {
            id: ScenarioNodeId(0),
            key: "hub".to_string(),
            role: NodeRole::Hub,
            tags: vec![],
            label: Some("Hub".to_string()),
            archetype_hint: None,
        }];

        let sub_graph = ScenarioGraph {
            nodes: vec![
                ScenarioNode {
                    id: ScenarioNodeId(0),
                    key: "entry".to_string(),
                    role: NodeRole::Entry,
                    tags: vec![],
                    label: Some("Sub Entry".to_string()),
                    archetype_hint: None,
                },
                ScenarioNode {
                    id: ScenarioNodeId(1),
                    key: "goal".to_string(),
                    role: NodeRole::Goal,
                    tags: vec![],
                    label: Some("Sub Goal".to_string()),
                    archetype_hint: None,
                },
            ],
            edges: vec![ScenarioEdge {
                from: ScenarioNodeId(0),
                to: ScenarioNodeId(1),
                role: EdgeRole::Traversal,
                tags: vec![],
            }],
        };

        let mut next_id: u32 = 1; // parent already used 0
        let _edges = graft_subgraph(&mut parent_nodes, "branch", sub_graph, &mut next_id);

        // Check that sub-graph keys are namespaced.
        let keys: Vec<&str> = parent_nodes.iter().map(|n| n.key.as_str()).collect();
        assert!(keys.contains(&"branch.entry"), "keys: {:?}", keys);
        assert!(keys.contains(&"branch.goal"), "keys: {:?}", keys);
        // Original parent node unchanged.
        assert!(keys.contains(&"hub"), "keys: {:?}", keys);
    }

    #[test]
    fn test_graft_subgraph_remaps_ids() {
        let mut parent_nodes = vec![
            ScenarioNode {
                id: ScenarioNodeId(0),
                key: "entry".to_string(),
                role: NodeRole::Entry,
                tags: vec![],
                label: None,
                archetype_hint: None,
            },
            ScenarioNode {
                id: ScenarioNodeId(1),
                key: "hub".to_string(),
                role: NodeRole::Hub,
                tags: vec![],
                label: None,
                archetype_hint: None,
            },
        ];

        let sub_graph = ScenarioGraph {
            nodes: vec![
                ScenarioNode {
                    id: ScenarioNodeId(0),
                    key: "entry".to_string(),
                    role: NodeRole::Entry,
                    tags: vec![],
                    label: None,
                    archetype_hint: None,
                },
                ScenarioNode {
                    id: ScenarioNodeId(1),
                    key: "mid".to_string(),
                    role: NodeRole::Hub,
                    tags: vec![],
                    label: None,
                    archetype_hint: None,
                },
                ScenarioNode {
                    id: ScenarioNodeId(2),
                    key: "goal".to_string(),
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
                    role: EdgeRole::Traversal,
                    tags: vec![],
                },
            ],
        };

        let mut next_id: u32 = 2; // parent used 0 and 1
        let edges = graft_subgraph(&mut parent_nodes, "branch", sub_graph, &mut next_id);

        // All node IDs must be unique.
        let ids: Vec<u32> = parent_nodes.iter().map(|n| n.id.0).collect();
        let id_set: HashSet<u32> = ids.iter().copied().collect();
        assert_eq!(ids.len(), id_set.len(), "duplicate IDs found: {:?}", ids);

        // next_id should have advanced by 3 (the sub-graph had 3 nodes).
        assert_eq!(next_id, 5);

        // Edges should reference valid (remapped) IDs.
        for edge in &edges {
            assert!(
                id_set.contains(&edge.from.0),
                "edge from {:?} not in node set",
                edge.from
            );
            assert!(
                id_set.contains(&edge.to.0),
                "edge to {:?} not in node set",
                edge.to
            );
        }
    }

    #[test]
    fn test_fill_composed_budget_zero_no_expansion() {
        let patterns = load_default_patterns().unwrap();
        let vocab = get_vocabulary();
        let situation = make_situation(&["sealed_crypt", "locked_vault"]);

        let parent_pattern = make_expandable_pattern();
        let all_patterns = {
            let mut p = patterns;
            p.push(parent_pattern.clone());
            p.push(make_child_pattern());
            p
        };

        let ctx = CompositionContext {
            patterns: &all_patterns,
            vocabulary: &vocab,
            situation: &situation,
            expansion_budget: 0, // No expansions allowed.
            excluded_pattern_ids: vec![],
        };

        let mut rng = StdRng::seed_from_u64(42);
        let result = fill_pattern_composed(&parent_pattern, &ctx, &mut rng).unwrap();

        // With budget=0, the expandable "branch" slot should be filled normally
        // (not expanded), so we expect exactly 3 nodes: entry, hub, branch.
        assert_eq!(
            result.graph.nodes.len(),
            3,
            "expected 3 nodes with budget=0, got {}",
            result.graph.nodes.len()
        );

        // No node key should contain a dot (no namespaced sub-graph nodes).
        for node in &result.graph.nodes {
            assert!(
                !node.key.contains('.'),
                "unexpected namespaced key '{}' with budget=0",
                node.key
            );
        }
    }

    #[test]
    fn test_fill_composed_with_expansion() {
        let vocab = get_vocabulary();
        let situation = make_situation(&["test"]);

        let parent_pattern = make_expandable_pattern();
        let child_pattern = make_child_pattern();
        let all_patterns = vec![parent_pattern.clone(), child_pattern];

        let ctx = CompositionContext {
            patterns: &all_patterns,
            vocabulary: &vocab,
            situation: &situation,
            expansion_budget: 3, // Generous budget.
            excluded_pattern_ids: vec!["test_parent".to_string()], // Exclude self.
        };

        // Try many seeds — at least one should produce an expansion.
        let mut expanded_any = false;
        for seed in 0..50 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = fill_pattern_composed(&parent_pattern, &ctx, &mut rng).unwrap();

            // If expansion happened, we should see namespaced keys.
            if result.graph.nodes.iter().any(|n| n.key.contains('.')) {
                expanded_any = true;
                // The expanded branch should produce nodes like "branch.entry", "branch.goal".
                assert!(
                    result
                        .graph
                        .nodes
                        .iter()
                        .any(|n| n.key.starts_with("branch.")),
                    "expected branch.* keys in expanded graph"
                );
                // Should have more than 3 nodes (parent's 2 normal + sub-graph's nodes).
                assert!(
                    result.graph.nodes.len() > 3,
                    "expected more than 3 nodes after expansion, got {}",
                    result.graph.nodes.len()
                );
                break;
            }
        }
        assert!(
            expanded_any,
            "expected at least one expansion across 50 seeds"
        );
    }

    #[test]
    fn test_expansion_excludes_self() {
        let vocab = get_vocabulary();
        let situation = make_situation(&["test"]);

        // Only pattern available is the parent itself.
        let parent_pattern = make_expandable_pattern();
        let all_patterns = vec![parent_pattern.clone()];

        let ctx = CompositionContext {
            patterns: &all_patterns,
            vocabulary: &vocab,
            situation: &situation,
            expansion_budget: 5,
            excluded_pattern_ids: vec!["test_parent".to_string()],
        };

        // Even with high budget, no expansion should happen (only pattern is excluded).
        for seed in 0..20 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = fill_pattern_composed(&parent_pattern, &ctx, &mut rng).unwrap();

            // No namespaced keys should exist — the slot should be filled normally.
            for node in &result.graph.nodes {
                assert!(
                    !node.key.contains('.'),
                    "seed {}: unexpected expansion with only self available: key='{}'",
                    seed,
                    node.key
                );
            }
        }
    }

    #[test]
    fn test_all_edges_reference_valid_nodes() {
        let vocab = get_vocabulary();
        let situation = make_situation(&["test"]);

        let parent_pattern = make_expandable_pattern();
        let child_pattern = make_child_pattern();
        let all_patterns = vec![parent_pattern.clone(), child_pattern];

        let ctx = CompositionContext {
            patterns: &all_patterns,
            vocabulary: &vocab,
            situation: &situation,
            expansion_budget: 3,
            excluded_pattern_ids: vec!["test_parent".to_string()],
        };

        for seed in 0..50 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = fill_pattern_composed(&parent_pattern, &ctx, &mut rng).unwrap();

            let node_ids: HashSet<ScenarioNodeId> =
                result.graph.nodes.iter().map(|n| n.id).collect();

            for edge in &result.graph.edges {
                assert!(
                    node_ids.contains(&edge.from),
                    "seed {}: edge from {:?} references missing node. nodes: {:?}",
                    seed,
                    edge.from,
                    result
                        .graph
                        .nodes
                        .iter()
                        .map(|n| (n.id, &n.key))
                        .collect::<Vec<_>>()
                );
                assert!(
                    node_ids.contains(&edge.to),
                    "seed {}: edge to {:?} references missing node. nodes: {:?}",
                    seed,
                    edge.to,
                    result
                        .graph
                        .nodes
                        .iter()
                        .map(|n| (n.id, &n.key))
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}
