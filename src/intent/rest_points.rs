//! Rest point insertion for composed graphs.
//!
//! After pattern composition produces a full `ScenarioGraph`, this module
//! analyzes the critical path and inserts rest point rooms where needed
//! to prevent monotonically rising tension without breathing room.
//!
//! Rest points are Transition-role nodes with the `"rest_point"` structural
//! tag, which the tension system forces to Low.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::intent::graph::{
    EdgeRole, NodeRole, ScenarioEdge, ScenarioGraph, ScenarioNode, ScenarioNodeId,
};
use crate::tag::Tag;

/// Minimum critical path length (in edges) before rest point insertion is considered.
const REST_POINT_PATH_THRESHOLD: u32 = 5;

/// Insert rest point rooms into the graph if the critical path is long
/// and lacks natural Low-tension rooms (Entry, Reward).
///
/// Algorithm:
/// 1. Find the Entry node and BFS to compute distances.
/// 2. Find the farthest Goal (critical path endpoint).
/// 3. Reconstruct the shortest path from Entry to Goal.
/// 4. If path length > threshold and no Entry/Reward exists mid-path,
///    insert a rest point by splitting a Traversal edge near the midpoint.
pub fn insert_rest_points(graph: &mut ScenarioGraph) {
    // Find entry node.
    let entry_id = match graph.nodes.iter().find(|n| n.role == NodeRole::Entry) {
        Some(n) => n.id,
        None => return,
    };

    // Build adjacency and BFS.
    let adj = build_adjacency(graph);
    let distances = bfs_distances(entry_id, &adj);

    // Find farthest Goal.
    let goal_id = graph
        .nodes
        .iter()
        .filter(|n| n.role == NodeRole::Goal)
        .max_by_key(|n| distances.get(&n.id).copied().unwrap_or(0))
        .map(|n| n.id);

    let Some(goal) = goal_id else { return };

    let critical_len = distances.get(&goal).copied().unwrap_or(0);

    // Only insert if path is long enough.
    if critical_len <= REST_POINT_PATH_THRESHOLD {
        return;
    }

    // Reconstruct the critical path.
    let path = find_shortest_path(entry_id, goal, &adj);
    if path.len() <= 2 {
        return;
    }

    // Check if there's already a natural rest point mid-path.
    // (Entry or Reward in the interior of the path.)
    let has_natural_rest = path[1..path.len() - 1].iter().any(|&id| {
        graph
            .nodes
            .iter()
            .find(|n| n.id == id)
            .is_some_and(|n| n.role == NodeRole::Entry || n.role == NodeRole::Reward)
    });

    if has_natural_rest {
        return;
    }

    // Find the best edge to split (Traversal edge near the midpoint).
    let midpoint_idx = path.len() / 2;
    let split_edge = find_splittable_edge(graph, &path, midpoint_idx);

    let Some((from_id, to_id, edge_idx)) = split_edge else {
        return;
    };

    // Insert the rest point node.
    let next_id = graph.nodes.iter().map(|n| n.id.0).max().unwrap_or(0) + 1;
    let rest_node = ScenarioNode {
        id: ScenarioNodeId(next_id),
        key: format!("rest_point_{}", next_id),
        role: NodeRole::Transition,
        tags: vec![Tag::from("rest_point")],
        label: Some("Safe Passage".to_string()),
        archetype_hint: None,
    };
    let rest_id = rest_node.id;
    graph.nodes.push(rest_node);

    // Replace the original edge with two edges through the rest point.
    let original_role = graph.edges[edge_idx].role;
    graph.edges.remove(edge_idx);
    graph.edges.push(ScenarioEdge {
        from: from_id,
        to: rest_id,
        role: EdgeRole::Traversal,
        tags: vec![],
    });
    graph.edges.push(ScenarioEdge {
        from: rest_id,
        to: to_id,
        role: original_role,
        tags: vec![],
    });
}

/// Find a Traversal edge to split, searching outward from the midpoint.
/// Returns (from_id, to_id, edge_index) or None.
fn find_splittable_edge(
    graph: &ScenarioGraph,
    path: &[ScenarioNodeId],
    midpoint_idx: usize,
) -> Option<(ScenarioNodeId, ScenarioNodeId, usize)> {
    // Try positions outward from midpoint.
    let max_offset = path.len() - 1;
    for offset in 0..max_offset {
        for &idx in &[midpoint_idx.wrapping_sub(offset), midpoint_idx + offset] {
            if idx == 0 || idx >= path.len() {
                continue;
            }
            let from = path[idx - 1];
            let to = path[idx];

            // Find a Traversal edge between these nodes.
            if let Some(edge_idx) = graph.edges.iter().position(|e| {
                e.role == EdgeRole::Traversal
                    && ((e.from == from && e.to == to) || (e.from == to && e.to == from))
            }) {
                return Some((from, to, edge_idx));
            }
        }
    }
    None
}

/// Build undirected adjacency from graph edges.
fn build_adjacency(graph: &ScenarioGraph) -> HashMap<ScenarioNodeId, Vec<ScenarioNodeId>> {
    let mut adj: HashMap<ScenarioNodeId, Vec<ScenarioNodeId>> = HashMap::new();
    for node in &graph.nodes {
        adj.entry(node.id).or_default();
    }
    for edge in &graph.edges {
        adj.entry(edge.from).or_default().push(edge.to);
        adj.entry(edge.to).or_default().push(edge.from);
    }
    adj
}

/// BFS distances from start.
fn bfs_distances(
    start: ScenarioNodeId,
    adj: &HashMap<ScenarioNodeId, Vec<ScenarioNodeId>>,
) -> HashMap<ScenarioNodeId, u32> {
    let mut distances: HashMap<ScenarioNodeId, u32> = HashMap::new();
    let mut queue: VecDeque<ScenarioNodeId> = VecDeque::new();
    let mut visited: HashSet<ScenarioNodeId> = HashSet::new();

    distances.insert(start, 0);
    visited.insert(start);
    queue.push_back(start);

    while let Some(current) = queue.pop_front() {
        let d = distances[&current];
        if let Some(neighbors) = adj.get(&current) {
            for &next in neighbors {
                if visited.insert(next) {
                    distances.insert(next, d + 1);
                    queue.push_back(next);
                }
            }
        }
    }

    distances
}

/// Reconstruct shortest path from start to end via BFS.
fn find_shortest_path(
    start: ScenarioNodeId,
    end: ScenarioNodeId,
    adj: &HashMap<ScenarioNodeId, Vec<ScenarioNodeId>>,
) -> Vec<ScenarioNodeId> {
    if start == end {
        return vec![start];
    }

    let mut visited: HashSet<ScenarioNodeId> = HashSet::new();
    let mut queue: VecDeque<ScenarioNodeId> = VecDeque::new();
    let mut parent: HashMap<ScenarioNodeId, ScenarioNodeId> = HashMap::new();

    visited.insert(start);
    queue.push_back(start);

    while let Some(current) = queue.pop_front() {
        if current == end {
            let mut path = vec![end];
            let mut node = end;
            while node != start {
                node = parent[&node];
                path.push(node);
            }
            path.reverse();
            return path;
        }

        if let Some(neighbors) = adj.get(&current) {
            for &next in neighbors {
                if visited.insert(next) {
                    parent.insert(next, current);
                    queue.push_back(next);
                }
            }
        }
    }

    Vec::new()
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: u32, role: NodeRole, key: &str) -> ScenarioNode {
        ScenarioNode {
            id: ScenarioNodeId(id),
            key: key.to_string(),
            role,
            tags: vec![],
            label: Some(key.to_string()),
            archetype_hint: None,
        }
    }

    fn make_edge(from: u32, to: u32, role: EdgeRole) -> ScenarioEdge {
        ScenarioEdge {
            from: ScenarioNodeId(from),
            to: ScenarioNodeId(to),
            role,
            tags: vec![],
        }
    }

    #[test]
    fn short_path_no_insertion() {
        // Entry → Hub → Gate → Goal (path = 3, below threshold)
        let mut graph = ScenarioGraph {
            nodes: vec![
                make_node(0, NodeRole::Entry, "entry"),
                make_node(1, NodeRole::Hub, "hub"),
                make_node(2, NodeRole::Gate, "gate"),
                make_node(3, NodeRole::Goal, "goal"),
            ],
            edges: vec![
                make_edge(0, 1, EdgeRole::Traversal),
                make_edge(1, 2, EdgeRole::Traversal),
                make_edge(2, 3, EdgeRole::RestrictedTraversal),
            ],
        };

        let original_count = graph.nodes.len();
        insert_rest_points(&mut graph);
        assert_eq!(
            graph.nodes.len(),
            original_count,
            "short path should not get rest point"
        );
    }

    #[test]
    fn long_path_gets_rest_point() {
        // Entry → T1 → T2 → T3 → T4 → T5 → Goal (path = 6, above threshold)
        let mut graph = ScenarioGraph {
            nodes: vec![
                make_node(0, NodeRole::Entry, "entry"),
                make_node(1, NodeRole::Transition, "t1"),
                make_node(2, NodeRole::Transition, "t2"),
                make_node(3, NodeRole::Transition, "t3"),
                make_node(4, NodeRole::Transition, "t4"),
                make_node(5, NodeRole::Transition, "t5"),
                make_node(6, NodeRole::Goal, "goal"),
            ],
            edges: vec![
                make_edge(0, 1, EdgeRole::Traversal),
                make_edge(1, 2, EdgeRole::Traversal),
                make_edge(2, 3, EdgeRole::Traversal),
                make_edge(3, 4, EdgeRole::Traversal),
                make_edge(4, 5, EdgeRole::Traversal),
                make_edge(5, 6, EdgeRole::Traversal),
            ],
        };

        insert_rest_points(&mut graph);

        // Should have inserted one rest point node.
        assert_eq!(graph.nodes.len(), 8, "should add one rest point node");

        // The rest point should have the "rest_point" tag.
        let rest_node = graph
            .nodes
            .iter()
            .find(|n| n.tags.contains(&Tag::from("rest_point")))
            .expect("should have a node with rest_point tag");
        assert_eq!(rest_node.role, NodeRole::Transition);
    }

    #[test]
    fn rest_point_not_inserted_when_reward_exists_mid_path() {
        // Entry → T1 → T2 → Reward → T3 → T4 → Goal
        // Path has a Reward mid-way — no insertion needed.
        let mut graph = ScenarioGraph {
            nodes: vec![
                make_node(0, NodeRole::Entry, "entry"),
                make_node(1, NodeRole::Transition, "t1"),
                make_node(2, NodeRole::Transition, "t2"),
                make_node(3, NodeRole::Reward, "reward"),
                make_node(4, NodeRole::Transition, "t3"),
                make_node(5, NodeRole::Transition, "t4"),
                make_node(6, NodeRole::Goal, "goal"),
            ],
            edges: vec![
                make_edge(0, 1, EdgeRole::Traversal),
                make_edge(1, 2, EdgeRole::Traversal),
                make_edge(2, 3, EdgeRole::Traversal),
                make_edge(3, 4, EdgeRole::Traversal),
                make_edge(4, 5, EdgeRole::Traversal),
                make_edge(5, 6, EdgeRole::Traversal),
            ],
        };

        let original_count = graph.nodes.len();
        insert_rest_points(&mut graph);
        assert_eq!(
            graph.nodes.len(),
            original_count,
            "path with existing rest should not get extra"
        );
    }

    #[test]
    fn rest_point_splits_traversal_edge_only() {
        // Entry → T1 → Gate(restricted) → T2 → T3 → T4 → Goal
        // The midpoint edge is the restricted traversal — should skip it
        // and split a nearby Traversal edge instead.
        let mut graph = ScenarioGraph {
            nodes: vec![
                make_node(0, NodeRole::Entry, "entry"),
                make_node(1, NodeRole::Transition, "t1"),
                make_node(2, NodeRole::Gate, "gate"),
                make_node(3, NodeRole::Transition, "t2"),
                make_node(4, NodeRole::Transition, "t3"),
                make_node(5, NodeRole::Transition, "t4"),
                make_node(6, NodeRole::Goal, "goal"),
            ],
            edges: vec![
                make_edge(0, 1, EdgeRole::Traversal),
                make_edge(1, 2, EdgeRole::Traversal),
                make_edge(2, 3, EdgeRole::RestrictedTraversal),
                make_edge(3, 4, EdgeRole::Traversal),
                make_edge(4, 5, EdgeRole::Traversal),
                make_edge(5, 6, EdgeRole::Traversal),
            ],
        };

        insert_rest_points(&mut graph);

        // Should still insert (found a non-restricted edge nearby).
        assert_eq!(graph.nodes.len(), 8);

        // The rest point should be connected via Traversal edges.
        let rest_node = graph
            .nodes
            .iter()
            .find(|n| n.tags.contains(&Tag::from("rest_point")))
            .unwrap();
        let rest_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.from == rest_node.id || e.to == rest_node.id)
            .collect();
        assert_eq!(rest_edges.len(), 2, "rest point should have 2 edges");
    }

    #[test]
    fn inserted_rest_point_preserves_graph_connectivity() {
        // Entry → Hub → T1 → T2 → T3 → Gate → Goal (path = 6)
        let mut graph = ScenarioGraph {
            nodes: vec![
                make_node(0, NodeRole::Entry, "entry"),
                make_node(1, NodeRole::Hub, "hub"),
                make_node(2, NodeRole::Transition, "t1"),
                make_node(3, NodeRole::Transition, "t2"),
                make_node(4, NodeRole::Transition, "t3"),
                make_node(5, NodeRole::Gate, "gate"),
                make_node(6, NodeRole::Goal, "goal"),
            ],
            edges: vec![
                make_edge(0, 1, EdgeRole::Traversal),
                make_edge(1, 2, EdgeRole::Traversal),
                make_edge(2, 3, EdgeRole::Traversal),
                make_edge(3, 4, EdgeRole::Traversal),
                make_edge(4, 5, EdgeRole::Traversal),
                make_edge(5, 6, EdgeRole::RestrictedTraversal),
            ],
        };

        insert_rest_points(&mut graph);

        // Verify all nodes are reachable from entry.
        let adj = build_adjacency(&graph);
        let distances = bfs_distances(ScenarioNodeId(0), &adj);
        for node in &graph.nodes {
            assert!(
                distances.contains_key(&node.id),
                "node {:?} should be reachable from entry",
                node.key
            );
        }
    }

    #[test]
    fn no_goal_no_insertion() {
        // Graph with no Goal — nothing to insert.
        let mut graph = ScenarioGraph {
            nodes: vec![
                make_node(0, NodeRole::Entry, "entry"),
                make_node(1, NodeRole::Hub, "hub"),
                make_node(2, NodeRole::Hub, "hub2"),
            ],
            edges: vec![
                make_edge(0, 1, EdgeRole::Traversal),
                make_edge(1, 2, EdgeRole::Traversal),
            ],
        };

        let original_count = graph.nodes.len();
        insert_rest_points(&mut graph);
        assert_eq!(graph.nodes.len(), original_count);
    }
}
