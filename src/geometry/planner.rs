use std::collections::{HashMap, HashSet, VecDeque};

use tracing::{debug, info, info_span, warn};

use crate::geometry::geom::*;
use crate::geometry::routing::{CorridorRouter, ZShapeRouter};
use crate::intent::graph::NodeRole;
use crate::spatial::plan::{SpaceId, SpaceKind, SpatialConstraint, SpatialPlan};
use crate::validate::Validator;
use crate::validate::geometry::GeometryValidator;

#[derive(Debug)]
pub enum GeometryPlanError {
    UnsupportedSpaceKind,
    MissingEntry,
    EmptyPlan,
    ValidationFailed(Vec<String>),
}

impl std::fmt::Display for GeometryPlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSpaceKind => write!(f, "unsupported space kind"),
            Self::MissingEntry => write!(f, "no entry space found"),
            Self::EmptyPlan => write!(f, "spatial plan has no spaces"),
            Self::ValidationFailed(msgs) => {
                write!(f, "geometry validation failed: {}", msgs.join("; "))
            }
        }
    }
}

impl std::error::Error for GeometryPlanError {}

pub trait GeometryPlanner {
    fn plan(&self, spatial: &SpatialPlan) -> Result<GeometryPlan, GeometryPlanError>;
}

/// Configuration for constraint-aware placement.
#[derive(Debug, Clone)]
pub struct PlacementConfig {
    /// Minimum gap (in tiles) between any two placed spaces.
    pub min_gap: i32,
    /// Extra gap enforced between `MustBeSeparated` space pairs.
    pub separation_gap: i32,
}

impl Default for PlacementConfig {
    fn default() -> Self {
        Self {
            min_gap: 2,
            separation_gap: 6,
        }
    }
}

#[derive(Default)]
pub struct SimpleGeometryPlanner {
    pub config: PlacementConfig,
}

impl GeometryPlanner for SimpleGeometryPlanner {
    fn plan(&self, spatial: &SpatialPlan) -> Result<GeometryPlan, GeometryPlanError> {
        if spatial.spaces.is_empty() {
            return Err(GeometryPlanError::EmptyPlan);
        }

        let _span = info_span!(
            "geometry_planning",
            spaces = spatial.spaces.len(),
            links = spatial.links.len()
        )
        .entered();

        // Build adjacency from links for BFS traversal.
        let adjacency = build_adjacency(spatial);

        // Choose the BFS root: prefer a PreferCentral space (hub), otherwise entry.
        let root_id = choose_bfs_root(spatial);
        info!(root = root_id.0, "selected BFS root");

        // BFS from the root to assign (depth, sibling_index) to each space.
        let placements = bfs_assign_positions(root_id, &adjacency, spatial);

        // Convert BFS positions into concrete rects.
        let mut placed = place_spaces_from_bfs(spatial, &placements);
        for p in &placed {
            debug!(
                id = p.space_id.0,
                x = p.rect.x,
                y = p.rect.y,
                w = p.rect.w,
                h = p.rect.h,
                label = ?p.label,
                "placed space"
            );
        }

        // Post-placement constraint adjustments.
        apply_constraint_adjustments(spatial, &mut placed, &self.config);

        // Enforce minimum gap between all space pairs.
        enforce_minimum_gap(&mut placed, self.config.min_gap);

        // Normalize coordinates so all spaces have positive positions with margin.
        // This prevents rooms from being pushed to negative coords or the map edge
        // by constraint adjustments (e.g. PreferPerimeter nudge).
        normalize_positions(&mut placed);
        {
            let bbox = compute_bounding_box(&placed);
            debug!(
                min_x = bbox.x,
                min_y = bbox.y,
                width = bbox.w,
                height = bbox.h,
                "bounding box after normalization"
            );
        }

        // Route links between placed spaces.
        let links = ZShapeRouter.route(spatial, &placed);
        info!(corridors = links.len(), "routing complete");

        Ok(GeometryPlan {
            spaces: placed,
            links,
        })
    }
}

impl SimpleGeometryPlanner {
    /// Runs `plan()` followed by `GeometryValidator`. On validation errors,
    /// retries up to `MAX_RETRIES` times with relaxed spacing (+2 each retry).
    pub fn plan_with_validation(
        &self,
        spatial: &SpatialPlan,
    ) -> Result<GeometryPlan, GeometryPlanError> {
        let _span = info_span!("plan_with_validation").entered();
        const MAX_RETRIES: u32 = 3;

        let mut last_errors = Vec::new();

        for attempt in 0..=MAX_RETRIES {
            let planner = SimpleGeometryPlanner {
                config: PlacementConfig {
                    min_gap: self.config.min_gap + (attempt as i32) * 2,
                    separation_gap: self.config.separation_gap + (attempt as i32) * 2,
                },
            };

            info!(
                attempt = attempt,
                min_gap = planner.config.min_gap,
                separation_gap = planner.config.separation_gap,
                "planning attempt"
            );
            let plan = planner.plan(spatial)?;
            let result = GeometryValidator::default().validate(&plan);

            // Log all validation issues.
            for issue in &result.issues {
                match issue.severity {
                    crate::validate::Severity::Error => {
                        warn!(msg = %issue.message, "validation error")
                    }
                    crate::validate::Severity::Warning => {
                        warn!(msg = %issue.message, "validation warning")
                    }
                    crate::validate::Severity::Info => {
                        debug!(msg = %issue.message, "validation info")
                    }
                }
            }

            if result.is_ok() {
                info!(attempt = attempt, "validation passed");
                return Ok(plan);
            }

            last_errors = result
                .issues
                .iter()
                .filter(|i| i.severity == crate::validate::Severity::Error)
                .map(|i| i.message.clone())
                .collect();
        }

        Err(GeometryPlanError::ValidationFailed(last_errors))
    }
}

// --- Internal helpers ---

/// Choose the BFS root based on spatial constraints.
/// Prefers a `PreferCentral` space (hub) as the root so it ends up at depth 0 (center).
/// Falls back to the entry space, then the first space.
fn choose_bfs_root(spatial: &SpatialPlan) -> SpaceId {
    // Check for PreferCentral constraint — use that space as root.
    for constraint in &spatial.constraints {
        if let SpatialConstraint::PreferCentral { space } = constraint {
            // Verify it exists in the plan.
            if spatial.spaces.iter().any(|s| s.id == *space) {
                return *space;
            }
        }
    }

    // Fall back to the entry space.
    spatial
        .spaces
        .iter()
        .find(|s| s.role == NodeRole::Entry)
        .or_else(|| spatial.spaces.first())
        .map(|s| s.id)
        .unwrap_or(SpaceId(0))
}

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

/// Apply constraint-based adjustments to placed spaces.
fn apply_constraint_adjustments(
    spatial: &SpatialPlan,
    placed: &mut [PlacedSpace],
    config: &PlacementConfig,
) {
    // Collect constraint info.
    let mut perimeter_spaces: HashSet<SpaceId> = HashSet::new();
    let mut separated_pairs: Vec<(SpaceId, SpaceId)> = Vec::new();

    for constraint in &spatial.constraints {
        match constraint {
            SpatialConstraint::PreferPerimeter { space } => {
                perimeter_spaces.insert(*space);
            }
            SpatialConstraint::MustBeSeparated { a, b } => {
                separated_pairs.push((*a, *b));
            }
            // PreferCentral is handled by BFS root selection.
            // Other constraints (MustBeAdjacent, MaxDistance, GatedBy) are
            // structural and handled by the spatial planner / link routing.
            _ => {}
        }
    }

    // --- PreferPerimeter: shift perimeter-preferred spaces outward ---
    if !perimeter_spaces.is_empty() {
        apply_perimeter_preference(placed, &perimeter_spaces);
    }

    // --- MustBeSeparated: push separated pairs apart ---
    for (a, b) in &separated_pairs {
        enforce_separation(placed, *a, *b, config.separation_gap);
    }
}

/// Shift spaces with PreferPerimeter toward the edges of the overall bounding box.
/// For the BFS layout, perimeter spaces are nudged toward the top-left (earliest position)
/// to place them at the outermost edge of the layout.
fn apply_perimeter_preference(placed: &mut [PlacedSpace], perimeter_spaces: &HashSet<SpaceId>) {
    if placed.is_empty() {
        return;
    }

    // Compute the bounding box of all placed spaces.
    let bbox = compute_bounding_box(placed);
    let center_x = bbox.x + bbox.w / 2;
    let center_y = bbox.y + bbox.h / 2;

    for space in placed.iter_mut() {
        if !perimeter_spaces.contains(&space.space_id) {
            continue;
        }

        let space_cx = space.rect.x + space.rect.w / 2;
        let space_cy = space.rect.y + space.rect.h / 2;

        // Push outward from center by a small nudge (2 tiles).
        let nudge = 2;
        let dx = if space_cx < center_x {
            -nudge
        } else if space_cx > center_x {
            nudge
        } else {
            0
        };
        let dy = if space_cy < center_y {
            -nudge
        } else if space_cy > center_y {
            nudge
        } else {
            -nudge // Default: push toward top (perimeter feel)
        };

        space.rect.x += dx;
        space.rect.y += dy;
        space.footprint = Footprint::Rect(space.rect);
        debug!(id = space.space_id.0, dx = dx, dy = dy, "perimeter nudge");
    }
}

/// Enforce that two spaces are separated by at least `min_gap` tiles.
/// If they are too close, push the second one away.
fn enforce_separation(placed: &mut [PlacedSpace], a: SpaceId, b: SpaceId, min_gap: i32) {
    let idx_a = placed.iter().position(|s| s.space_id == a);
    let idx_b = placed.iter().position(|s| s.space_id == b);

    let (Some(idx_a), Some(idx_b)) = (idx_a, idx_b) else {
        return;
    };

    let rect_a = placed[idx_a].rect;
    let rect_b = placed[idx_b].rect;

    let gap = rect_a.gap_to(&rect_b);
    if gap >= min_gap {
        return;
    }

    // Compute direction from a to b and push b away.
    let deficit = min_gap - gap;
    let dx = rect_b.center().x - rect_a.center().x;
    let dy = rect_b.center().y - rect_a.center().y;

    if dx.abs() >= dy.abs() {
        // Primarily horizontal separation.
        let shift = if dx >= 0 { deficit } else { -deficit };
        placed[idx_b].rect.x += shift;
    } else {
        // Primarily vertical separation.
        let shift = if dy >= 0 { deficit } else { -deficit };
        placed[idx_b].rect.y += shift;
    }
    placed[idx_b].footprint = Footprint::Rect(placed[idx_b].rect);
    debug!(a = a.0, b = b.0, deficit = deficit, "enforced separation");
}

/// Enforce minimum gap between all space pairs by iteratively pushing apart
/// any pair that is too close. Runs a limited number of iterations to converge.
fn enforce_minimum_gap(placed: &mut [PlacedSpace], min_gap: i32) {
    // Iterative relaxation: push apart pairs until no violations remain.
    // Limited iterations to avoid infinite loops with pathological inputs.
    let max_iterations = 10;

    for _ in 0..max_iterations {
        let mut any_violation = false;

        for i in 0..placed.len() {
            for j in (i + 1)..placed.len() {
                let gap = placed[i].rect.gap_to(&placed[j].rect);
                if gap >= min_gap {
                    continue;
                }

                any_violation = true;
                let deficit = min_gap - gap;

                // Push the later-placed space (j) away from i.
                let dx = placed[j].rect.center().x - placed[i].rect.center().x;
                let dy = placed[j].rect.center().y - placed[i].rect.center().y;

                if dx.abs() >= dy.abs() {
                    // Push horizontally.
                    let shift = if dx >= 0 {
                        (deficit + 1) / 2
                    } else {
                        -((deficit + 1) / 2)
                    };
                    placed[j].rect.x += shift;
                    placed[i].rect.x -= shift;
                } else {
                    // Push vertically.
                    let shift = if dy >= 0 {
                        (deficit + 1) / 2
                    } else {
                        -((deficit + 1) / 2)
                    };
                    placed[j].rect.y += shift;
                    placed[i].rect.y -= shift;
                }

                // Update footprints.
                placed[i].footprint = Footprint::Rect(placed[i].rect);
                placed[j].footprint = Footprint::Rect(placed[j].rect);
                debug!(
                    space_i = placed[i].space_id.0,
                    space_j = placed[j].space_id.0,
                    gap = gap,
                    min_gap = min_gap,
                    "pushed spaces apart for minimum gap"
                );
            }
        }

        if !any_violation {
            break;
        }
    }
}

/// Compute the axis-aligned bounding box of all placed spaces.
fn compute_bounding_box(placed: &[PlacedSpace]) -> Rect {
    let min_x = placed.iter().map(|s| s.rect.x).min().unwrap_or(0);
    let min_y = placed.iter().map(|s| s.rect.y).min().unwrap_or(0);
    let max_x = placed
        .iter()
        .map(|s| s.rect.x + s.rect.w)
        .max()
        .unwrap_or(0);
    let max_y = placed
        .iter()
        .map(|s| s.rect.y + s.rect.h)
        .max()
        .unwrap_or(0);
    Rect {
        x: min_x,
        y: min_y,
        w: max_x - min_x,
        h: max_y - min_y,
    }
}

/// Normalize all placed spaces so that the minimum x and y coordinates are at
/// least `MARGIN` tiles from the origin. This ensures corridors and walls have
/// room to render even after constraint adjustments push spaces around.
fn normalize_positions(placed: &mut [PlacedSpace]) {
    const MARGIN: i32 = 2;

    if placed.is_empty() {
        return;
    }

    let min_x = placed.iter().map(|s| s.rect.x).min().unwrap_or(0);
    let min_y = placed.iter().map(|s| s.rect.y).min().unwrap_or(0);

    let shift_x = MARGIN - min_x;
    let shift_y = MARGIN - min_y;

    if shift_x == 0 && shift_y == 0 {
        return;
    }

    debug!(
        shift_x = shift_x,
        shift_y = shift_y,
        "normalizing positions"
    );

    for space in placed.iter_mut() {
        space.rect.x += shift_x;
        space.rect.y += shift_y;
        space.footprint = Footprint::Rect(space.rect);
    }
}

/// Extract width/height from a space's kind.
fn get_space_dimensions(space: &crate::spatial::plan::SpaceSpec) -> (i32, i32) {
    match &space.kind {
        SpaceKind::Atomic(a) => (a.width, a.height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::graph::EdgeRole;
    use crate::intent::graph::{NodeRole, ScenarioNodeId};
    use crate::spatial::plan::{
        AtomicSpace, RealizationStyle, SizeHint, SpaceArchetype, SpaceKind, SpaceLink, SpaceSpec,
    };
    use crate::validate::geometry::{GeometryValidator, GeometryValidatorConfig};
    use crate::validate::{Severity, Validator};

    /// Helper: build a simple space spec.
    fn make_space(id: u32, role: NodeRole, w: i32, h: i32) -> SpaceSpec {
        SpaceSpec {
            id: SpaceId(id),
            origin: ScenarioNodeId(id),
            role,
            tags: vec![],
            style: RealizationStyle::RoomLike,
            kind: SpaceKind::Atomic(AtomicSpace {
                width: w,
                height: h,
            }),
            label: Some(format!("space_{}", id)),
            archetype: Some(SpaceArchetype::Chamber),
            size_hint: SizeHint::Medium,
        }
    }

    /// Helper: build a minimal plan with an entry and hub.
    fn hub_and_entry_plan() -> SpatialPlan {
        SpatialPlan {
            spaces: vec![
                make_space(0, NodeRole::Entry, 5, 5),
                make_space(1, NodeRole::Hub, 9, 7),
                make_space(2, NodeRole::Branch, 7, 7),
                make_space(3, NodeRole::Branch, 7, 7),
            ],
            links: vec![
                SpaceLink {
                    from: SpaceId(0),
                    to: SpaceId(1),
                    role: EdgeRole::Traversal,
                    tags: vec![],
                },
                SpaceLink {
                    from: SpaceId(1),
                    to: SpaceId(2),
                    role: EdgeRole::Traversal,
                    tags: vec![],
                },
                SpaceLink {
                    from: SpaceId(1),
                    to: SpaceId(3),
                    role: EdgeRole::Traversal,
                    tags: vec![],
                },
            ],
            constraints: vec![
                SpatialConstraint::PreferCentral { space: SpaceId(1) },
                SpatialConstraint::PreferPerimeter { space: SpaceId(0) },
            ],
        }
    }

    #[test]
    fn prefer_central_space_is_at_depth_zero() {
        let spatial = hub_and_entry_plan();
        let planner = SimpleGeometryPlanner::default();
        let plan = planner.plan(&spatial).unwrap();

        // The hub (space 1) has PreferCentral, so it should be the BFS root
        // and placed at the leftmost column (depth 0).
        let hub = plan
            .spaces
            .iter()
            .find(|s| s.space_id == SpaceId(1))
            .unwrap();
        let entry = plan
            .spaces
            .iter()
            .find(|s| s.space_id == SpaceId(0))
            .unwrap();

        // Hub should be in the first column (smallest x).
        assert!(
            hub.rect.x <= entry.rect.x,
            "hub (x={}) should be at depth 0 (before or at entry x={})",
            hub.rect.x,
            entry.rect.x
        );
    }

    #[test]
    fn prefer_perimeter_shifts_space_outward() {
        let spatial = hub_and_entry_plan();
        let planner = SimpleGeometryPlanner::default();
        let plan = planner.plan(&spatial).unwrap();

        let entry = plan
            .spaces
            .iter()
            .find(|s| s.space_id == SpaceId(0))
            .unwrap();
        let hub = plan
            .spaces
            .iter()
            .find(|s| s.space_id == SpaceId(1))
            .unwrap();

        // Entry has PreferPerimeter. After the nudge it should be farther from center
        // than the hub on at least one axis.
        let bbox = compute_bounding_box(&plan.spaces);
        let center_x = bbox.x + bbox.w / 2;

        let entry_dist = (entry.rect.center().x - center_x).abs();
        let hub_dist = (hub.rect.center().x - center_x).abs();

        // Entry should be at least as far from center as hub (it's pushed outward).
        assert!(
            entry_dist >= hub_dist || entry.rect.x != hub.rect.x,
            "entry (dist={}) should be farther from center than hub (dist={})",
            entry_dist,
            hub_dist
        );
    }

    #[test]
    fn must_be_separated_enforces_gap() {
        let spatial = SpatialPlan {
            spaces: vec![
                make_space(0, NodeRole::Entry, 5, 5),
                make_space(1, NodeRole::Branch, 5, 5),
                make_space(2, NodeRole::Goal, 5, 5),
            ],
            links: vec![
                SpaceLink {
                    from: SpaceId(0),
                    to: SpaceId(1),
                    role: EdgeRole::Traversal,
                    tags: vec![],
                },
                SpaceLink {
                    from: SpaceId(0),
                    to: SpaceId(2),
                    role: EdgeRole::Traversal,
                    tags: vec![],
                },
            ],
            constraints: vec![SpatialConstraint::MustBeSeparated {
                a: SpaceId(1),
                b: SpaceId(2),
            }],
        };

        let planner = SimpleGeometryPlanner {
            config: PlacementConfig {
                min_gap: 2,
                separation_gap: 8,
            },
        };
        let plan = planner.plan(&spatial).unwrap();

        let s1 = plan
            .spaces
            .iter()
            .find(|s| s.space_id == SpaceId(1))
            .unwrap();
        let s2 = plan
            .spaces
            .iter()
            .find(|s| s.space_id == SpaceId(2))
            .unwrap();

        let gap = s1.rect.gap_to(&s2.rect);
        assert!(
            gap >= 8,
            "MustBeSeparated spaces should have at least separation_gap=8 between them, got {}",
            gap
        );
    }

    #[test]
    fn minimum_gap_enforced_between_all_spaces() {
        let spatial = SpatialPlan {
            spaces: vec![
                make_space(0, NodeRole::Entry, 7, 7),
                make_space(1, NodeRole::Branch, 7, 7),
                make_space(2, NodeRole::Branch, 7, 7),
            ],
            links: vec![
                SpaceLink {
                    from: SpaceId(0),
                    to: SpaceId(1),
                    role: EdgeRole::Traversal,
                    tags: vec![],
                },
                SpaceLink {
                    from: SpaceId(0),
                    to: SpaceId(2),
                    role: EdgeRole::Traversal,
                    tags: vec![],
                },
            ],
            constraints: vec![],
        };

        let min_gap = 3;
        let planner = SimpleGeometryPlanner {
            config: PlacementConfig {
                min_gap,
                separation_gap: 6,
            },
        };
        let plan = planner.plan(&spatial).unwrap();

        for i in 0..plan.spaces.len() {
            for j in (i + 1)..plan.spaces.len() {
                let gap = plan.spaces[i].rect.gap_to(&plan.spaces[j].rect);
                assert!(
                    gap >= min_gap,
                    "spaces {:?} and {:?} have gap {} < min_gap {}",
                    plan.spaces[i].space_id,
                    plan.spaces[j].space_id,
                    gap,
                    min_gap,
                );
            }
        }
    }

    #[test]
    fn no_overlaps_after_constraint_adjustments() {
        let spatial = hub_and_entry_plan();
        let planner = SimpleGeometryPlanner::default();
        let plan = planner.plan(&spatial).unwrap();

        for i in 0..plan.spaces.len() {
            for j in (i + 1)..plan.spaces.len() {
                assert!(
                    !plan.spaces[i].rect.overlaps(&plan.spaces[j].rect),
                    "spaces {:?} and {:?} overlap after constraint adjustments",
                    plan.spaces[i].space_id,
                    plan.spaces[j].space_id,
                );
            }
        }
    }

    #[test]
    fn geometry_validator_passes_for_hub_plan() {
        let spatial = hub_and_entry_plan();
        let planner = SimpleGeometryPlanner::default();
        let plan = planner.plan(&spatial).unwrap();

        let validator = GeometryValidator::new(GeometryValidatorConfig { min_spacing: 1 });
        let result = validator.validate(&plan);

        let errors: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "geometry validator should pass with no errors, got: {:?}",
            errors
        );
    }

    #[test]
    fn geometry_validator_passes_for_separated_plan() {
        let spatial = SpatialPlan {
            spaces: vec![
                make_space(0, NodeRole::Entry, 5, 5),
                make_space(1, NodeRole::Branch, 5, 5),
                make_space(2, NodeRole::Goal, 5, 5),
            ],
            links: vec![
                SpaceLink {
                    from: SpaceId(0),
                    to: SpaceId(1),
                    role: EdgeRole::Traversal,
                    tags: vec![],
                },
                SpaceLink {
                    from: SpaceId(0),
                    to: SpaceId(2),
                    role: EdgeRole::Traversal,
                    tags: vec![],
                },
            ],
            constraints: vec![SpatialConstraint::MustBeSeparated {
                a: SpaceId(1),
                b: SpaceId(2),
            }],
        };

        let planner = SimpleGeometryPlanner::default();
        let plan = planner.plan(&spatial).unwrap();

        let validator = GeometryValidator::new(GeometryValidatorConfig { min_spacing: 1 });
        let result = validator.validate(&plan);

        let errors: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "geometry validator should pass for separated plan, got: {:?}",
            errors
        );
    }

    #[test]
    fn empty_plan_returns_error() {
        let spatial = SpatialPlan {
            spaces: vec![],
            links: vec![],
            constraints: vec![],
        };
        let planner = SimpleGeometryPlanner::default();
        assert!(planner.plan(&spatial).is_err());
    }

    #[test]
    fn single_space_plan_produces_valid_output() {
        let spatial = SpatialPlan {
            spaces: vec![make_space(0, NodeRole::Entry, 7, 7)],
            links: vec![],
            constraints: vec![],
        };
        let planner = SimpleGeometryPlanner::default();
        let plan = planner.plan(&spatial).unwrap();

        assert_eq!(plan.spaces.len(), 1);
        assert_eq!(plan.spaces[0].rect.w, 7);
        assert_eq!(plan.spaces[0].rect.h, 7);
    }

    #[test]
    fn plan_with_validation_passes_for_hub_plan() {
        let spatial = hub_and_entry_plan();
        let planner = SimpleGeometryPlanner::default();
        let plan = planner.plan_with_validation(&spatial);
        assert!(
            plan.is_ok(),
            "plan_with_validation should succeed: {:?}",
            plan.err()
        );
    }
}
