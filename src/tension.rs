//! Tension curve calculation for pacing.
//!
//! Assigns a [`TensionLevel`] to each room based on its structural position
//! relative to the critical path (shortest Entry → Goal path in the graph).
//! Tension is a computed property, not a tag — it reflects structural depth,
//! not content.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::intent::graph::NodeRole;
use crate::spatial::plan::{SpaceId, SpaceLink, SpaceSpec, SpatialPlan};
use crate::tag::Tag;

/// Discrete tension level assigned to a room based on its pacing position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TensionLevel {
    /// Safe areas, rest points, entry zones. Sparse enemies (minions only).
    Low,
    /// Moderate challenge. Standard encounters.
    Medium,
    /// High challenge. Stronger/more enemies, tighter spaces.
    High,
    /// Boss area, final challenge before the goal. Maximum intensity.
    Climax,
}

impl std::fmt::Display for TensionLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "Low"),
            Self::Medium => write!(f, "Medium"),
            Self::High => write!(f, "High"),
            Self::Climax => write!(f, "Climax"),
        }
    }
}

impl TensionLevel {
    /// Returns the entity density multiplier for this tension level.
    ///
    /// Higher tension means more entities can be placed in a room.
    /// - Low: 0.5× (sparse)
    /// - Medium: 1.0× (standard)
    /// - High: 1.5× (elevated)
    /// - Climax: 2.0× (maximum)
    pub fn density_multiplier(&self) -> f64 {
        match self {
            Self::Low => 0.5,
            Self::Medium => 1.0,
            Self::High => 1.5,
            Self::Climax => 2.0,
        }
    }
}

/// Computes the tension level for each room in the spatial plan.
///
/// Algorithm:
/// 1. Find the root Entry room (first `NodeRole::Entry`).
/// 2. Build an adjacency map from `SpatialPlan.links`.
/// 3. BFS from Entry to find shortest distances to all rooms.
/// 4. Find the **farthest** Goal room (by BFS distance) as the critical path
///    endpoint. If a room has the `main_goal` tag, that one is preferred
///    regardless of distance. This ensures that composed/expanded graphs
///    stretch the tension curve across their full depth.
/// 5. Assign tension based on each room's distance normalized against the critical path:
///    - Root Entry (depth 0): forced Low
///    - All Reward rooms: forced Low (rest/loot points)
///    - All Goal rooms: forced Climax
///    - Transition rooms: capped at Medium (passages/corridors)
///    - Hub rooms: capped at High (crossroads)
///    - Gate and Branch: uncapped depth-based
///    - depth ≤ 25%: Low, ≤60%: Medium, ≤90%: High, >90%: Climax
///
/// Sub-pattern Entry nodes are converted to Transition during grafting
/// (see `compose.rs`), so they receive the Transition cap (Medium).
///
/// Rooms not on any path from entry (disconnected) get `Low` by default.
/// If no Goal is found, tension is assigned purely by depth from entry with
/// no normalization cap.
pub fn compute_tension(spatial: &SpatialPlan) -> HashMap<SpaceId, TensionLevel> {
    let mut result: HashMap<SpaceId, TensionLevel> = HashMap::new();

    // Find root entry (first Entry-role room).
    let entry_id = spatial
        .spaces
        .iter()
        .find(|s| s.role == NodeRole::Entry)
        .map(|s| s.id);

    let Some(entry) = entry_id else {
        // No entry found — assign Low to everything.
        for space in &spatial.spaces {
            result.insert(space.id, TensionLevel::Low);
        }
        return result;
    };

    // Build adjacency (undirected for BFS distance).
    let adjacency = build_adjacency(&spatial.links, &spatial.spaces);

    // BFS from entry to compute distances.
    let distances = bfs_distances(entry, &adjacency);

    // Find the primary goal — the one that defines the critical path.
    let primary_goal = find_primary_goal(spatial, &distances);

    // Determine critical path length.
    let critical_len = primary_goal
        .and_then(|g| distances.get(&g).copied())
        .unwrap_or_else(|| {
            // No goal or goal unreachable — use max distance as fallback.
            distances.values().copied().max().unwrap_or(0)
        });

    // Assign tension levels.
    for space in &spatial.spaces {
        let dist = distances.get(&space.id).copied().unwrap_or(0);
        let level = assign_tension_level(dist, critical_len, space, primary_goal);
        result.insert(space.id, level);
    }

    result
}

/// Finds the primary Goal room that defines the critical path endpoint.
///
/// Strategy:
/// 1. If any room has the `main_goal` structural tag, use that (explicit override).
/// 2. Otherwise, pick the **farthest** Goal room by BFS distance from entry.
///    This ensures that in composed/expanded graphs, the critical path spans
///    the full depth — preventing near-Goal rooms from making the critical
///    path artificially short and turning most rooms into Climax.
/// 3. Falls back to None if no Goal exists.
fn find_primary_goal(spatial: &SpatialPlan, distances: &HashMap<SpaceId, u32>) -> Option<SpaceId> {
    use crate::tag::Tag;

    // Prefer the room tagged "main_goal" (explicit override).
    if let Some(s) = spatial
        .spaces
        .iter()
        .find(|s| s.structural_tags.contains(&Tag::from("main_goal")))
    {
        return Some(s.id);
    }

    // Find the farthest Goal room by BFS distance.
    spatial
        .spaces
        .iter()
        .filter(|s| s.role == NodeRole::Goal)
        .max_by_key(|s| distances.get(&s.id).copied().unwrap_or(0))
        .map(|s| s.id)
}

/// Build undirected adjacency map from spatial links.
fn build_adjacency(links: &[SpaceLink], spaces: &[SpaceSpec]) -> HashMap<SpaceId, Vec<SpaceId>> {
    let mut adj: HashMap<SpaceId, Vec<SpaceId>> = HashMap::new();
    for space in spaces {
        adj.entry(space.id).or_default();
    }
    for link in links {
        adj.entry(link.from).or_default().push(link.to);
        adj.entry(link.to).or_default().push(link.from);
    }
    adj
}

/// BFS from a source, returning distance to each reachable node.
fn bfs_distances(
    start: SpaceId,
    adjacency: &HashMap<SpaceId, Vec<SpaceId>>,
) -> HashMap<SpaceId, u32> {
    let mut distances: HashMap<SpaceId, u32> = HashMap::new();
    let mut queue: VecDeque<SpaceId> = VecDeque::new();
    let mut visited: HashSet<SpaceId> = HashSet::new();

    distances.insert(start, 0);
    visited.insert(start);
    queue.push_back(start);

    while let Some(current) = queue.pop_front() {
        let current_dist = distances[&current];
        if let Some(neighbors) = adjacency.get(&current) {
            for &next in neighbors {
                if visited.insert(next) {
                    distances.insert(next, current_dist + 1);
                    queue.push_back(next);
                }
            }
        }
    }

    distances
}

/// Assign tension level based on a room's depth, the critical path length,
/// and the room's role.
///
/// Role-based overrides (rest points):
/// - **Root Entry (depth 0)**: forced Low (player starting point).
/// - **All Goal rooms**: always Climax (major objectives).
/// - **Reward rooms**: always Low (rest/loot points).
/// - **Transition rooms**: capped at Medium (passages provide breathing room).
/// - **Hub rooms**: capped at High (crossroads, not boss chambers).
/// - **Gate and Branch**: uncapped depth-based tension.
fn assign_tension_level(
    depth: u32,
    critical_len: u32,
    space: &SpaceSpec,
    _primary_goal: Option<SpaceId>,
) -> TensionLevel {
    // All Goal rooms are Climax (they're major objectives).
    if space.role == NodeRole::Goal {
        return TensionLevel::Climax;
    }

    // Only the root Entry (depth 0) is forced Low — it's the player's
    // starting point. Sub-pattern entries are converted to Transition during
    // grafting, so they won't reach this check.
    if space.role == NodeRole::Entry && depth == 0 {
        return TensionLevel::Low;
    }

    // Reward rooms are always Low (rest/loot points).
    if space.role == NodeRole::Reward {
        return TensionLevel::Low;
    }

    // Rooms with the "rest_point" structural tag are forced Low.
    // These are inserted by the rest point post-processing step.
    if space.structural_tags.contains(&Tag::from("rest_point")) {
        return TensionLevel::Low;
    }

    // If critical path length is 0 or 1, everything is Low.
    if critical_len <= 1 {
        return TensionLevel::Low;
    }

    // Normalize depth against critical path.
    let ratio = depth as f64 / critical_len as f64;

    let base_level = if ratio <= 0.25 {
        TensionLevel::Low
    } else if ratio <= 0.60 {
        TensionLevel::Medium
    } else if ratio <= 0.90 {
        TensionLevel::High
    } else {
        TensionLevel::Climax
    };

    // Role-based caps — provide pacing valleys (rest points).
    // Transition rooms are passages/corridors: cap at Medium.
    if space.role == NodeRole::Transition {
        return std::cmp::min(base_level, TensionLevel::Medium);
    }

    // Hub rooms are crossroads: cap at High.
    if space.role == NodeRole::Hub {
        return std::cmp::min(base_level, TensionLevel::High);
    }

    base_level
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::graph::{EdgeRole, ScenarioNodeId};
    use crate::spatial::plan::*;
    use crate::tag::Tag;

    /// Helper: build a minimal SpaceSpec.
    fn make_space(id: u32, role: NodeRole, tags: &[&str]) -> SpaceSpec {
        let raw_tags: Vec<Tag> = tags.iter().map(|s| Tag::from(*s)).collect();
        let (structural, atmosphere) = classify_tags(&raw_tags);
        SpaceSpec {
            id: SpaceId(id),
            origin: ScenarioNodeId(id),
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
            archetype: None,
            size_hint: SizeHint::Medium,
            max_connectors: None,
            connector_distribution: None,
        }
    }

    fn make_link(from: u32, to: u32) -> SpaceLink {
        SpaceLink {
            from: SpaceId(from),
            to: SpaceId(to),
            role: EdgeRole::Traversal,
            tags: vec![],
        }
    }

    #[test]
    fn linear_path_assigns_increasing_tension() {
        // Entry(0) → Hub(1) → Gate(2) → Hub(3) → Goal(4)
        let spatial = SpatialPlan {
            spaces: vec![
                make_space(0, NodeRole::Entry, &[]),
                make_space(1, NodeRole::Hub, &[]),
                make_space(2, NodeRole::Gate, &[]),
                make_space(3, NodeRole::Hub, &[]),
                make_space(4, NodeRole::Goal, &["main_goal"]),
            ],
            links: vec![
                make_link(0, 1),
                make_link(1, 2),
                make_link(2, 3),
                make_link(3, 4),
            ],
            constraints: vec![],
            location_kind: crate::intent::map_intent::LocationKind::Dungeon,
        };

        let tension = compute_tension(&spatial);

        assert_eq!(tension[&SpaceId(0)], TensionLevel::Low); // Entry: always Low
        assert_eq!(tension[&SpaceId(1)], TensionLevel::Low); // depth 1/4 = 0.25 -> Low
        assert_eq!(tension[&SpaceId(2)], TensionLevel::Medium); // depth 2/4 = 0.50 -> Medium
        assert_eq!(tension[&SpaceId(3)], TensionLevel::High); // depth 3/4 = 0.75 -> High
        assert_eq!(tension[&SpaceId(4)], TensionLevel::Climax); // Goal: always Climax
    }

    #[test]
    fn short_path_assigns_reasonable_tension() {
        // Entry(0) → Goal(1) — critical path length = 1
        let spatial = SpatialPlan {
            spaces: vec![
                make_space(0, NodeRole::Entry, &[]),
                make_space(1, NodeRole::Goal, &["main_goal"]),
            ],
            links: vec![make_link(0, 1)],
            constraints: vec![],
            location_kind: crate::intent::map_intent::LocationKind::Dungeon,
        };

        let tension = compute_tension(&spatial);

        assert_eq!(tension[&SpaceId(0)], TensionLevel::Low);
        assert_eq!(tension[&SpaceId(1)], TensionLevel::Climax);
    }

    #[test]
    fn reward_rooms_are_always_low() {
        // Entry(0) → Hub(1) → Reward(2) → Goal(3)
        let spatial = SpatialPlan {
            spaces: vec![
                make_space(0, NodeRole::Entry, &[]),
                make_space(1, NodeRole::Hub, &[]),
                make_space(2, NodeRole::Reward, &[]),
                make_space(3, NodeRole::Goal, &["main_goal"]),
            ],
            links: vec![make_link(0, 1), make_link(1, 2), make_link(2, 3)],
            constraints: vec![],
            location_kind: crate::intent::map_intent::LocationKind::Dungeon,
        };

        let tension = compute_tension(&spatial);

        // Reward at depth 2/3 ≈ 0.67 would normally be High, but Reward overrides to Low.
        assert_eq!(tension[&SpaceId(2)], TensionLevel::Low);
    }

    #[test]
    fn branch_off_critical_path_gets_tension_by_depth() {
        // Entry(0) → Hub(1) → Gate(2) → Goal(3)
        //                 └→ Branch(4)
        let spatial = SpatialPlan {
            spaces: vec![
                make_space(0, NodeRole::Entry, &[]),
                make_space(1, NodeRole::Hub, &[]),
                make_space(2, NodeRole::Gate, &[]),
                make_space(3, NodeRole::Goal, &["main_goal"]),
                make_space(4, NodeRole::Branch, &[]),
            ],
            links: vec![
                make_link(0, 1),
                make_link(1, 2),
                make_link(2, 3),
                make_link(1, 4),
            ],
            constraints: vec![],
            location_kind: crate::intent::map_intent::LocationKind::Dungeon,
        };

        let tension = compute_tension(&spatial);

        // Branch(4) is at distance 2 from entry (0→1→4).
        // Critical path length = 3 (0→1→2→3).
        // Ratio = 2/3 = 0.667, which is > 0.60, so High.
        assert_eq!(tension[&SpaceId(4)], TensionLevel::High);
    }

    #[test]
    fn no_goal_uses_max_depth() {
        // Entry(0) → Hub(1) → Hub(2) → Branch(3)
        // No Goal room at all.
        let spatial = SpatialPlan {
            spaces: vec![
                make_space(0, NodeRole::Entry, &[]),
                make_space(1, NodeRole::Hub, &[]),
                make_space(2, NodeRole::Hub, &[]),
                make_space(3, NodeRole::Branch, &[]),
            ],
            links: vec![make_link(0, 1), make_link(1, 2), make_link(2, 3)],
            constraints: vec![],
            location_kind: crate::intent::map_intent::LocationKind::Dungeon,
        };

        let tension = compute_tension(&spatial);

        // Max depth is 3 (0→1→2→3). No goal override.
        assert_eq!(tension[&SpaceId(0)], TensionLevel::Low); // Entry
        assert_eq!(tension[&SpaceId(1)], TensionLevel::Medium); // 1/3 = 0.33 → Medium
        assert_eq!(tension[&SpaceId(2)], TensionLevel::High); // 2/3 = 0.67 → High
        assert_eq!(tension[&SpaceId(3)], TensionLevel::Climax); // 3/3 = 1.0 → Climax
    }

    #[test]
    fn tension_level_display() {
        assert_eq!(format!("{}", TensionLevel::Low), "Low");
        assert_eq!(format!("{}", TensionLevel::Medium), "Medium");
        assert_eq!(format!("{}", TensionLevel::High), "High");
        assert_eq!(format!("{}", TensionLevel::Climax), "Climax");
    }

    #[test]
    fn tension_level_ordering() {
        assert!(TensionLevel::Low < TensionLevel::Medium);
        assert!(TensionLevel::Medium < TensionLevel::High);
        assert!(TensionLevel::High < TensionLevel::Climax);
        // Transitive
        assert!(TensionLevel::Low < TensionLevel::Climax);
    }

    #[test]
    fn tension_level_density_multiplier() {
        assert_eq!(TensionLevel::Low.density_multiplier(), 0.5);
        assert_eq!(TensionLevel::Medium.density_multiplier(), 1.0);
        assert_eq!(TensionLevel::High.density_multiplier(), 1.5);
        assert_eq!(TensionLevel::Climax.density_multiplier(), 2.0);
    }

    #[test]
    fn long_linear_path_has_gradual_buildup() {
        // 8 rooms in a straight line: Entry → 6 Hubs → Goal
        let mut spaces = vec![make_space(0, NodeRole::Entry, &[])];
        for i in 1..7 {
            spaces.push(make_space(i, NodeRole::Hub, &[]));
        }
        spaces.push(make_space(7, NodeRole::Goal, &["main_goal"]));

        let links: Vec<SpaceLink> = (0..7).map(|i| make_link(i, i + 1)).collect();

        let spatial = SpatialPlan {
            spaces,
            links,
            constraints: vec![],
            location_kind: crate::intent::map_intent::LocationKind::Dungeon,
        };

        let tension = compute_tension(&spatial);

        // Critical path length = 7.
        // depth 0: Entry → Low
        // depth 1: 1/7 = 0.14 → Low
        // depth 2: 2/7 = 0.29 → Medium
        // depth 3: 3/7 = 0.43 → Medium
        // depth 4: 4/7 = 0.57 → Medium
        // depth 5: 5/7 = 0.71 → High
        // depth 6: 6/7 = 0.86 → High
        // depth 7: Goal → Climax
        assert_eq!(tension[&SpaceId(0)], TensionLevel::Low);
        assert_eq!(tension[&SpaceId(1)], TensionLevel::Low);
        assert_eq!(tension[&SpaceId(2)], TensionLevel::Medium);
        assert_eq!(tension[&SpaceId(3)], TensionLevel::Medium);
        assert_eq!(tension[&SpaceId(4)], TensionLevel::Medium);
        assert_eq!(tension[&SpaceId(5)], TensionLevel::High);
        assert_eq!(tension[&SpaceId(6)], TensionLevel::High);
        assert_eq!(tension[&SpaceId(7)], TensionLevel::Climax);
    }

    #[test]
    fn transition_rooms_capped_at_medium() {
        // Entry(0) → Transition(1) → Transition(2) → Transition(3) → Goal(4)
        // Critical path length = 4.
        // Without cap: depth 3/4=0.75 → High, depth 2/4=0.5 → Medium
        // With cap: Transition capped at Medium regardless.
        let spatial = SpatialPlan {
            spaces: vec![
                make_space(0, NodeRole::Entry, &[]),
                make_space(1, NodeRole::Transition, &[]),
                make_space(2, NodeRole::Transition, &[]),
                make_space(3, NodeRole::Transition, &[]),
                make_space(4, NodeRole::Goal, &["main_goal"]),
            ],
            links: vec![
                make_link(0, 1),
                make_link(1, 2),
                make_link(2, 3),
                make_link(3, 4),
            ],
            constraints: vec![],
            location_kind: crate::intent::map_intent::LocationKind::Dungeon,
        };

        let tension = compute_tension(&spatial);

        assert_eq!(tension[&SpaceId(0)], TensionLevel::Low); // Entry
        assert_eq!(tension[&SpaceId(1)], TensionLevel::Low); // Transition, 1/4=0.25 → Low (below cap)
        assert_eq!(tension[&SpaceId(2)], TensionLevel::Medium); // Transition, 2/4=0.5 → Medium (at cap)
        assert_eq!(tension[&SpaceId(3)], TensionLevel::Medium); // Transition, 3/4=0.75 → would be High, capped to Medium
        assert_eq!(tension[&SpaceId(4)], TensionLevel::Climax); // Goal
    }

    #[test]
    fn hub_rooms_capped_at_high() {
        // Build a long chain where a Hub would reach Climax territory without the cap:
        // Entry(0) → Gate(1) → ... → Gate(9) → Hub(10) → Goal(11)
        // Critical path = 11. Hub(10): 10/11=0.91 → Climax, capped at High!
        let mut spaces = vec![make_space(0, NodeRole::Entry, &[])];
        for i in 1..=9 {
            spaces.push(make_space(i, NodeRole::Gate, &[]));
        }
        spaces.push(make_space(10, NodeRole::Hub, &[]));
        spaces.push(make_space(11, NodeRole::Goal, &["main_goal"]));

        let links: Vec<SpaceLink> = (0..11).map(|i| make_link(i, i + 1)).collect();

        let spatial = SpatialPlan {
            spaces,
            links,
            constraints: vec![],
            location_kind: crate::intent::map_intent::LocationKind::Dungeon,
        };

        let tension = compute_tension(&spatial);

        // Hub at depth 10/11 ≈ 0.91 → would be Climax, but Hub is capped at High.
        assert_eq!(tension[&SpaceId(10)], TensionLevel::High);
        // Gate at depth 9: 9/11=0.818 → High (uncapped).
        assert_eq!(tension[&SpaceId(9)], TensionLevel::High);
        // Goal is always Climax.
        assert_eq!(tension[&SpaceId(11)], TensionLevel::Climax);
    }

    #[test]
    fn grafted_sub_pattern_entry_becomes_transition() {
        // After grafting, sub-pattern entries become Transition nodes.
        // Entry(0) → Hub(1) → Gate(2) → Transition(3) → Transition(4) → Goal(5)
        // The grafted Transition(3) at depth 3 is capped at Medium.
        let spatial = SpatialPlan {
            spaces: vec![
                make_space(0, NodeRole::Entry, &[]),
                make_space(1, NodeRole::Hub, &[]),
                make_space(2, NodeRole::Gate, &[]),
                make_space(3, NodeRole::Transition, &[]), // grafted sub-pattern entry
                make_space(4, NodeRole::Transition, &[]),
                make_space(5, NodeRole::Goal, &["main_goal"]),
            ],
            links: vec![
                make_link(0, 1),
                make_link(1, 2),
                make_link(2, 3),
                make_link(3, 4),
                make_link(4, 5),
            ],
            constraints: vec![],
            location_kind: crate::intent::map_intent::LocationKind::Dungeon,
        };

        let tension = compute_tension(&spatial);

        assert_eq!(tension[&SpaceId(0)], TensionLevel::Low); // Root entry
        // Transition(3): depth 3/5=0.6 → Medium, capped at Medium → Medium
        assert_eq!(tension[&SpaceId(3)], TensionLevel::Medium);
        // Transition(4): depth 4/5=0.8 → High, capped at Medium → Medium
        assert_eq!(tension[&SpaceId(4)], TensionLevel::Medium);
        assert_eq!(tension[&SpaceId(5)], TensionLevel::Climax); // Goal
    }

    #[test]
    fn gate_and_branch_are_uncapped() {
        // Entry(0) → Branch(1) → Gate(2) → Gate(3) → Goal(4)
        // Critical path = 4.
        // Gate(3): 3/4=0.75 → High (no cap)
        // Gate(2): 2/4=0.5 → Medium (no cap)
        // Branch(1): 1/4=0.25 → Low (no cap, just low depth)
        let spatial = SpatialPlan {
            spaces: vec![
                make_space(0, NodeRole::Entry, &[]),
                make_space(1, NodeRole::Branch, &[]),
                make_space(2, NodeRole::Gate, &[]),
                make_space(3, NodeRole::Gate, &[]),
                make_space(4, NodeRole::Goal, &["main_goal"]),
            ],
            links: vec![
                make_link(0, 1),
                make_link(1, 2),
                make_link(2, 3),
                make_link(3, 4),
            ],
            constraints: vec![],
            location_kind: crate::intent::map_intent::LocationKind::Dungeon,
        };

        let tension = compute_tension(&spatial);

        assert_eq!(tension[&SpaceId(1)], TensionLevel::Low); // Branch at 1/4=0.25 → Low
        assert_eq!(tension[&SpaceId(2)], TensionLevel::Medium); // Gate at 2/4=0.5 → Medium
        assert_eq!(tension[&SpaceId(3)], TensionLevel::High); // Gate at 3/4=0.75 → High
        assert_eq!(tension[&SpaceId(4)], TensionLevel::Climax); // Goal
    }
}
