use std::collections::{HashMap, HashSet, VecDeque};

use crate::geometry::geom::*;
use crate::geometry::routing::{CorridorRouter, ZShapeRouter};
use crate::intent::graph::NodeRole;
use crate::spatial::plan::{SpaceId, SpaceKind, SpatialPlan};

#[derive(Debug)]
pub enum GeometryPlanError {
    UnsupportedSpaceKind,
    MissingEntry,
    EmptyPlan,
}

impl std::fmt::Display for GeometryPlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSpaceKind => write!(f, "unsupported space kind"),
            Self::MissingEntry => write!(f, "no entry space found"),
            Self::EmptyPlan => write!(f, "spatial plan has no spaces"),
        }
    }
}

impl std::error::Error for GeometryPlanError {}

pub trait GeometryPlanner {
    fn plan(&self, spatial: &SpatialPlan) -> Result<GeometryPlan, GeometryPlanError>;
}

#[derive(Default)]
pub struct SimpleGeometryPlanner;

impl GeometryPlanner for SimpleGeometryPlanner {
    fn plan(&self, spatial: &SpatialPlan) -> Result<GeometryPlan, GeometryPlanError> {
        if spatial.spaces.is_empty() {
            return Err(GeometryPlanError::EmptyPlan);
        }

        // Build adjacency from links for BFS traversal.
        let adjacency = build_adjacency(spatial);

        // Find the entry space as our starting point.
        let entry_id = spatial
            .spaces
            .iter()
            .find(|s| s.role == NodeRole::Entry)
            .or_else(|| spatial.spaces.first())
            .ok_or(GeometryPlanError::EmptyPlan)?
            .id;

        // BFS from the entry to assign (depth, sibling_index) to each space.
        let placements = bfs_assign_positions(entry_id, &adjacency, spatial);

        // Convert BFS positions into concrete rects.
        let placed = place_spaces_from_bfs(spatial, &placements);

        // Route links between placed spaces.
        let links = ZShapeRouter.route(spatial, &placed);

        Ok(GeometryPlan {
            spaces: placed,
            links,
        })
    }
}

// --- Internal helpers ---

/// Adjacency list from the spatial plan's links.
fn build_adjacency(spatial: &SpatialPlan) -> HashMap<SpaceId, Vec<SpaceId>> {
    let mut adj: HashMap<SpaceId, Vec<SpaceId>> = HashMap::new();
    for link in &spatial.links {
        adj.entry(link.from).or_default().push(link.to);
        // Also store reverse for undirected BFS traversal
        adj.entry(link.to).or_default().push(link.from);
    }
    adj
}

/// BFS position assignment: each space gets a (depth, sibling_index).
struct BfsPosition {
    depth: i32,
    sibling: i32,
}

fn bfs_assign_positions(
    start: SpaceId,
    adjacency: &HashMap<SpaceId, Vec<SpaceId>>,
    spatial: &SpatialPlan,
) -> HashMap<SpaceId, BfsPosition> {
    let mut positions = HashMap::new();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    visited.insert(start);
    queue.push_back((start, 0i32));
    positions.insert(
        start,
        BfsPosition {
            depth: 0,
            sibling: 0,
        },
    );

    let mut depth_counters: HashMap<i32, i32> = HashMap::new();
    depth_counters.insert(0, 1);

    while let Some((current, depth)) = queue.pop_front() {
        if let Some(neighbors) = adjacency.get(&current) {
            for &neighbor in neighbors {
                if visited.contains(&neighbor) {
                    continue;
                }
                visited.insert(neighbor);
                let next_depth = depth + 1;
                let sibling = depth_counters.entry(next_depth).or_insert(0);
                positions.insert(
                    neighbor,
                    BfsPosition {
                        depth: next_depth,
                        sibling: *sibling,
                    },
                );
                *sibling += 1;
                queue.push_back((neighbor, next_depth));
            }
        }
    }

    // Handle any disconnected spaces (shouldn't happen in valid plans, but be safe).
    for space in &spatial.spaces {
        if let std::collections::hash_map::Entry::Vacant(e) = positions.entry(space.id) {
            let sibling = depth_counters.entry(0).or_insert(0);
            e.insert(BfsPosition {
                depth: 0,
                sibling: *sibling,
            });
            *sibling += 1;
        }
    }

    positions
}

/// The horizontal gap between rooms at different depths.
const DEPTH_SPACING: i32 = 4;
/// The vertical gap between sibling rooms at the same depth.
const SIBLING_SPACING: i32 = 4;

/// Convert BFS positions into placed rects.
fn place_spaces_from_bfs(
    spatial: &SpatialPlan,
    positions: &HashMap<SpaceId, BfsPosition>,
) -> Vec<PlacedSpace> {
    // First pass: compute the max width at each depth level so we can align columns.
    let mut depth_max_width: HashMap<i32, i32> = HashMap::new();
    for space in &spatial.spaces {
        let (w, _) = get_space_dimensions(space);
        let pos = &positions[&space.id];
        let current_max = depth_max_width.entry(pos.depth).or_insert(0);
        *current_max = (*current_max).max(w);
    }

    // Compute x-offset for each depth column.
    let max_depth = positions.values().map(|p| p.depth).max().unwrap_or(0);
    let mut depth_x_offsets = Vec::new();
    let mut x_cursor = 2; // start with a small margin
    for d in 0..=max_depth {
        depth_x_offsets.push(x_cursor);
        let col_width = depth_max_width.get(&d).copied().unwrap_or(9);
        x_cursor += col_width + DEPTH_SPACING;
    }

    // Compute the number of siblings at each depth for vertical centering.
    let mut depth_siblings: HashMap<i32, Vec<SpaceId>> = HashMap::new();
    for space in &spatial.spaces {
        let pos = &positions[&space.id];
        depth_siblings.entry(pos.depth).or_default().push(space.id);
    }
    // Sort siblings by their sibling index.
    for siblings in depth_siblings.values_mut() {
        siblings.sort_by_key(|id| positions[id].sibling);
    }

    // Second pass: assign y positions per depth column.
    let mut placed = Vec::new();
    for space in &spatial.spaces {
        let pos = &positions[&space.id];
        let (w, h) = get_space_dimensions(space);

        let x = depth_x_offsets[pos.depth as usize];

        // Compute y: stack siblings vertically with spacing.
        let siblings = &depth_siblings[&pos.depth];
        let mut y = 2; // top margin
        for &sib_id in siblings {
            if sib_id == space.id {
                break;
            }
            let sib_space = spatial.spaces.iter().find(|s| s.id == sib_id).unwrap();
            let (_, sib_h) = get_space_dimensions(sib_space);
            y += sib_h + SIBLING_SPACING;
        }

        let rect = Rect { x, y, w, h };
        placed.push(PlacedSpace {
            space_id: space.id,
            rect,
            footprint: Footprint::Rect(rect),
            style: space.style,
            label: space.label.clone(),
        });
    }

    placed
}

/// Extract width/height from a space's kind.
fn get_space_dimensions(space: &crate::spatial::plan::SpaceSpec) -> (i32, i32) {
    match &space.kind {
        SpaceKind::Atomic(a) => (a.width, a.height),
    }
}
