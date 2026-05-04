//! Force-directed geometry planner.
//!
//! Uses a physics-inspired simulation to layout rooms:
//! - **Attraction** along graph edges (connected rooms pull toward each other)
//! - **Repulsion** between all non-overlapping pairs (rooms push apart)
//! - **Overlap resolution** (hard constraint: no two rooms may share cells)
//!
//! Produces more natural spatial relationships than the BFS column layout,
//! especially for larger room counts (10–15+).

use std::collections::{HashMap, HashSet};

use rand::Rng;
use tracing::{debug, info, info_span, trace};

use crate::geometry::geom::*;
use crate::geometry::planner::{GeometryPlanError, GeometryPlanner, PlacementConfig};
use crate::geometry::routing::{CorridorRouter, ZShapeRouter};
use crate::geometry::shape::select_shape_refinement;
use crate::intent::map_intent::LocationKind;
use crate::spatial::plan::{SpaceId, SpaceKind, SpatialConstraint, SpatialPlan};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Tuning knobs for the force-directed simulation.
#[derive(Debug, Clone)]
pub struct ForceDirectedConfig {
    /// Placement configuration (min_gap, separation_gap).
    pub placement: PlacementConfig,

    /// Number of simulation iterations.
    pub iterations: u32,

    /// Strength of attraction along graph edges.
    /// Higher values pull connected rooms closer together.
    pub attraction_strength: f64,

    /// Desired edge-to-edge spacing (corridor length) between connected rooms.
    /// The actual center-to-center ideal distance is computed per-pair as:
    ///   `(w_a + w_b) / 2 + ideal_spacing`
    /// This ensures the simulation accounts for room sizes.
    pub ideal_spacing: f64,

    /// Strength of repulsion between non-adjacent rooms.
    /// Higher values push unconnected rooms further apart.
    pub repulsion_strength: f64,

    /// Maximum force magnitude per iteration (prevents instability).
    pub max_force: f64,

    /// Damping factor applied each iteration (0..1). Lower = more damping.
    pub damping: f64,

    /// Cooling factor: multiplied into damping each iteration to settle.
    pub cooling: f64,
}

impl Default for ForceDirectedConfig {
    fn default() -> Self {
        Self {
            placement: PlacementConfig::default(),
            iterations: 200,
            attraction_strength: 1.5,
            ideal_spacing: 3.0,
            repulsion_strength: 50.0,
            max_force: 8.0,
            damping: 0.85,
            cooling: 0.98,
        }
    }
}

// ---------------------------------------------------------------------------
// Planner
// ---------------------------------------------------------------------------

/// A force-directed geometry planner that uses physics simulation
/// to produce natural spatial layouts for room graphs.
#[derive(Default)]
pub struct ForceDirectedGeometryPlanner {
    pub config: ForceDirectedConfig,
}

impl GeometryPlanner for ForceDirectedGeometryPlanner {
    fn plan(
        &self,
        spatial: &SpatialPlan,
        rng: &mut dyn rand::RngCore,
    ) -> Result<GeometryPlan, GeometryPlanError> {
        if spatial.spaces.is_empty() {
            return Err(GeometryPlanError::EmptyPlan);
        }

        let _span = info_span!(
            "force_directed_planning",
            spaces = spatial.spaces.len(),
            links = spatial.links.len()
        )
        .entered();

        // ── 1. Initial placement: random positions with some structure ──
        let mut positions = initial_placement(spatial, rng);

        // ── 2. Build edge set for attraction ──
        let edges = build_edge_set(spatial);

        // ── 3. Collect constraint info ──
        let constraints = collect_constraints(spatial);

        // ── 4. Get dimensions for each space ──
        let dimensions: HashMap<SpaceId, (i32, i32)> = spatial
            .spaces
            .iter()
            .map(|s| {
                let (w, h) = get_space_dimensions(s);
                (s.id, (w, h))
            })
            .collect();

        // ── 5. Run force-directed simulation ──
        info!(
            iterations = self.config.iterations,
            "starting force simulation"
        );
        self.simulate(&mut positions, &edges, &dimensions, &constraints);

        // ── 6. Snap to integer grid and resolve overlaps ──
        let mut placed = snap_to_grid(spatial, &positions, &dimensions);

        // ── 7. Resolve overlaps iteratively ──
        resolve_overlaps(&mut placed, self.config.placement.min_gap);

        // ── 8. Apply constraint adjustments ──
        apply_force_constraints(&mut placed, &constraints, &self.config.placement);

        // ── 9. Normalize positions ──
        normalize_positions(&mut placed);

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

        // ── 10. Shape refinement ──
        let location_kind = spatial.location_kind;
        apply_shape_refinement(&mut placed, spatial, location_kind, rng);

        // ── 11. Route corridors ──
        let links = ZShapeRouter.route(spatial, &placed);
        info!(corridors = links.len(), "routing complete");

        Ok(GeometryPlan {
            spaces: placed,
            links,
        })
    }
}

impl ForceDirectedGeometryPlanner {
    /// Run the force-directed simulation.
    fn simulate(
        &self,
        positions: &mut HashMap<SpaceId, (f64, f64)>,
        edges: &HashSet<(SpaceId, SpaceId)>,
        dimensions: &HashMap<SpaceId, (i32, i32)>,
        constraints: &ConstraintInfo,
    ) {
        let ids: Vec<SpaceId> = positions.keys().copied().collect();
        let mut damping = self.config.damping;

        // Log initial state
        {
            let mut total_edge_dist = 0.0;
            let mut edge_count = 0;
            for &(id_a, id_b) in edges {
                let (ax, ay) = positions[&id_a];
                let (bx, by) = positions[&id_b];
                let dist = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
                total_edge_dist += dist;
                edge_count += 1;
            }
            let avg_edge = if edge_count > 0 {
                total_edge_dist / edge_count as f64
            } else {
                0.0
            };
            debug!(
                avg_initial_edge_dist = format!("{:.1}", avg_edge),
                nodes = ids.len(),
                edges = edge_count,
                "simulation initial state"
            );
        }

        for iter in 0..self.config.iterations {
            let mut forces: HashMap<SpaceId, (f64, f64)> = HashMap::new();
            for &id in &ids {
                forces.insert(id, (0.0, 0.0));
            }

            // ── Repulsion: all pairs push apart (global, falls off with d²) ──
            for i in 0..ids.len() {
                for j in (i + 1)..ids.len() {
                    let id_a = ids[i];
                    let id_b = ids[j];

                    let (ax, ay) = positions[&id_a];
                    let (bx, by) = positions[&id_b];

                    let dx = bx - ax;
                    let dy = by - ay;
                    let dist_sq = dx * dx + dy * dy;

                    // Minimum distance based on room sizes to avoid division near zero
                    let (wa, ha) = dimensions[&id_a];
                    let (wb, hb) = dimensions[&id_b];
                    let min_dist = ((wa + wb) as f64 / 2.0).max((ha + hb) as f64 / 2.0);
                    let min_dist_sq = (min_dist * 0.5).powi(2);
                    let effective_dist_sq = dist_sq.max(min_dist_sq);

                    // Extra repulsion for separated pairs
                    let repulsion_mult = if constraints.separated_pairs.contains(&(id_a, id_b))
                        || constraints.separated_pairs.contains(&(id_b, id_a))
                    {
                        5.0
                    } else {
                        1.0
                    };

                    let force_mag =
                        self.config.repulsion_strength * repulsion_mult / effective_dist_sq;

                    let dist = effective_dist_sq.sqrt();
                    let fx = force_mag * dx / dist;
                    let fy = force_mag * dy / dist;

                    // a is pushed away from b (negative direction)
                    forces.get_mut(&id_a).unwrap().0 -= fx;
                    forces.get_mut(&id_a).unwrap().1 -= fy;
                    // b is pushed away from a (positive direction)
                    forces.get_mut(&id_b).unwrap().0 += fx;
                    forces.get_mut(&id_b).unwrap().1 += fy;
                }
            }

            // ── Attraction: connected pairs pull together ──
            for &(id_a, id_b) in edges {
                let (ax, ay) = positions[&id_a];
                let (bx, by) = positions[&id_b];

                let dx = bx - ax;
                let dy = by - ay;
                let dist = (dx * dx + dy * dy).sqrt().max(1.0);

                // Compute per-pair ideal center-to-center distance based on room sizes.
                let (wa, ha) = dimensions[&id_a];
                let (wb, hb) = dimensions[&id_b];
                let avg_size = ((wa + wb) as f64 / 2.0).max((ha + hb) as f64 / 2.0);
                let ideal_dist = avg_size + self.config.ideal_spacing;

                // Spring force: pull toward ideal distance
                let displacement = dist - ideal_dist;
                let force_mag = self.config.attraction_strength * displacement;

                let fx = force_mag * dx / dist;
                let fy = force_mag * dy / dist;

                // a is pulled toward b
                forces.get_mut(&id_a).unwrap().0 += fx;
                forces.get_mut(&id_a).unwrap().1 += fy;
                // b is pulled toward a
                forces.get_mut(&id_b).unwrap().0 -= fx;
                forces.get_mut(&id_b).unwrap().1 -= fy;
            }

            // ── Central gravity for PreferCentral nodes ──
            if !constraints.central_spaces.is_empty() {
                // Compute centroid
                let n = positions.len() as f64;
                let (cx, cy) = positions
                    .values()
                    .fold((0.0, 0.0), |(sx, sy), &(x, y)| (sx + x / n, sy + y / n));

                for &id in &constraints.central_spaces {
                    if let Some((px, py)) = positions.get(&id) {
                        let dx = cx - px;
                        let dy = cy - py;
                        let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                        let force_mag = 0.5 * dist; // Gentle pull toward center
                        let fx = force_mag * dx / dist;
                        let fy = force_mag * dy / dist;
                        forces.get_mut(&id).unwrap().0 += fx;
                        forces.get_mut(&id).unwrap().1 += fy;
                    }
                }
            }

            // ── Perimeter push for PreferPerimeter nodes ──
            if !constraints.perimeter_spaces.is_empty() {
                let n = positions.len() as f64;
                let (cx, cy) = positions
                    .values()
                    .fold((0.0, 0.0), |(sx, sy), &(x, y)| (sx + x / n, sy + y / n));

                for &id in &constraints.perimeter_spaces {
                    if let Some((px, py)) = positions.get(&id) {
                        let dx = px - cx;
                        let dy = py - cy;
                        let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                        // Push away from center
                        let force_mag = 0.8;
                        let fx = force_mag * dx / dist;
                        let fy = force_mag * dy / dist;
                        forces.get_mut(&id).unwrap().0 += fx;
                        forces.get_mut(&id).unwrap().1 += fy;
                    }
                }
            }

            // ── Apply forces with damping and max force clamping ──
            let mut max_displacement = 0.0f64;
            for &id in &ids {
                let (fx, fy) = forces[&id];
                let mag = (fx * fx + fy * fy).sqrt();
                let (clamped_fx, clamped_fy) = if mag > self.config.max_force {
                    let scale = self.config.max_force / mag;
                    (fx * scale, fy * scale)
                } else {
                    (fx, fy)
                };

                let dx = clamped_fx * damping;
                let dy = clamped_fy * damping;

                let pos = positions.get_mut(&id).unwrap();
                pos.0 += dx;
                pos.1 += dy;

                max_displacement = max_displacement.max((dx * dx + dy * dy).sqrt());
            }

            // Cool down
            damping *= self.config.cooling;

            // Periodic logging
            if iter % 50 == 0 || iter == self.config.iterations - 1 {
                let mut total_edge_dist = 0.0;
                let mut max_edge_dist = 0.0f64;
                let mut edge_count = 0;
                for &(id_a, id_b) in edges {
                    let (ax, ay) = positions[&id_a];
                    let (bx, by) = positions[&id_b];
                    let dist = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
                    total_edge_dist += dist;
                    max_edge_dist = max_edge_dist.max(dist);
                    edge_count += 1;
                }
                let avg_edge = if edge_count > 0 {
                    total_edge_dist / edge_count as f64
                } else {
                    0.0
                };
                trace!(
                    iter = iter,
                    max_disp = format!("{:.2}", max_displacement),
                    damping = format!("{:.3}", damping),
                    avg_edge_dist = format!("{:.1}", avg_edge),
                    max_edge_dist = format!("{:.1}", max_edge_dist),
                    "simulation progress"
                );
            }

            // Early termination if system has settled
            if max_displacement < 0.1 {
                debug!(
                    iter = iter,
                    max_displacement = format!("{:.3}", max_displacement),
                    "simulation converged early"
                );
                break;
            }
        }

        // Final edge distance summary
        {
            let mut min_edge = f64::MAX;
            let mut max_edge = 0.0f64;
            let mut total_edge = 0.0;
            let mut count = 0;
            for &(id_a, id_b) in edges {
                let (ax, ay) = positions[&id_a];
                let (bx, by) = positions[&id_b];
                let dist = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
                let (wa, ha) = dimensions[&id_a];
                let (wb, hb) = dimensions[&id_b];
                let avg_size = ((wa + wb) as f64 / 2.0).max((ha + hb) as f64 / 2.0);
                let ideal = avg_size + self.config.ideal_spacing;
                trace!(
                    edge = format!("{}->{}", id_a.0, id_b.0),
                    dist = format!("{:.1}", dist),
                    ideal = format!("{:.1}", ideal),
                    delta = format!("{:+.1}", dist - ideal),
                    "edge distance"
                );
                min_edge = min_edge.min(dist);
                max_edge = max_edge.max(dist);
                total_edge += dist;
                count += 1;
            }
            if count > 0 {
                debug!(
                    avg_edge_dist = format!("{:.1}", total_edge / count as f64),
                    min_edge_dist = format!("{:.1}", min_edge),
                    max_edge_dist = format!("{:.1}", max_edge),
                    "simulation complete"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Constraint info extracted from the spatial plan.
struct ConstraintInfo {
    central_spaces: HashSet<SpaceId>,
    perimeter_spaces: HashSet<SpaceId>,
    separated_pairs: HashSet<(SpaceId, SpaceId)>,
}

fn collect_constraints(spatial: &SpatialPlan) -> ConstraintInfo {
    let mut central_spaces = HashSet::new();
    let mut perimeter_spaces = HashSet::new();
    let mut separated_pairs = HashSet::new();

    for constraint in &spatial.constraints {
        match constraint {
            SpatialConstraint::PreferCentral { space } => {
                central_spaces.insert(*space);
            }
            SpatialConstraint::PreferPerimeter { space } => {
                perimeter_spaces.insert(*space);
            }
            SpatialConstraint::MustBeSeparated { a, b } => {
                separated_pairs.insert((*a, *b));
            }
            _ => {}
        }
    }

    ConstraintInfo {
        central_spaces,
        perimeter_spaces,
        separated_pairs,
    }
}

/// Build a set of edges (undirected) from the spatial plan's links.
fn build_edge_set(spatial: &SpatialPlan) -> HashSet<(SpaceId, SpaceId)> {
    let mut edges = HashSet::new();
    for link in &spatial.links {
        // Store in canonical order (smaller id first) to avoid duplicates
        let (a, b) = if link.from.0 <= link.to.0 {
            (link.from, link.to)
        } else {
            (link.to, link.from)
        };
        edges.insert((a, b));
    }
    edges
}

/// Initial placement using a radial layout with some randomness.
/// The BFS root (hub or entry) starts at center; others are placed in a spiral.
/// Radius is derived from room sizes to start close to the ideal configuration.
fn initial_placement(
    spatial: &SpatialPlan,
    rng: &mut dyn rand::RngCore,
) -> HashMap<SpaceId, (f64, f64)> {
    let mut positions = HashMap::new();

    if spatial.spaces.is_empty() {
        return positions;
    }

    // Compute average room size to scale initial placement
    let avg_room_size: f64 = if spatial.spaces.is_empty() {
        7.0
    } else {
        let total: f64 = spatial
            .spaces
            .iter()
            .map(|s| {
                let (w, h) = get_space_dimensions(s);
                w.max(h) as f64
            })
            .sum();
        total / spatial.spaces.len() as f64
    };
    // Initial radius: slightly larger than room size so rooms don't start overlapping
    let base_radius = avg_room_size + 2.0;

    // Find the root space (PreferCentral or entry)
    let root_id = find_root(spatial);

    // Place root at origin
    positions.insert(root_id, (0.0, 0.0));

    // BFS from root to assign initial positions radially
    let adjacency = build_adjacency(spatial);
    let mut visited = HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    visited.insert(root_id);
    queue.push_back(root_id);

    while let Some(current) = queue.pop_front() {
        let (cx, cy) = positions[&current];
        let neighbors: Vec<SpaceId> = adjacency.get(&current).cloned().unwrap_or_default();

        let neighbor_count = neighbors.iter().filter(|n| !visited.contains(n)).count();
        if neighbor_count == 0 {
            continue;
        }

        // Spread unvisited neighbors around the current node
        let angle_step = std::f64::consts::TAU / neighbor_count.max(1) as f64;
        let base_angle: f64 = rng.r#gen::<f64>() * std::f64::consts::TAU;
        let radius = base_radius + rng.r#gen::<f64>() * 3.0; // Small jitter

        let mut idx = 0;
        for &neighbor in &neighbors {
            if visited.contains(&neighbor) {
                continue;
            }
            visited.insert(neighbor);

            let angle = base_angle + angle_step * idx as f64;
            let nx = cx + radius * angle.cos();
            let ny = cy + radius * angle.sin();
            positions.insert(neighbor, (nx, ny));

            queue.push_back(neighbor);
            idx += 1;
        }
    }

    // Handle disconnected spaces (place them at random positions far out)
    for space in &spatial.spaces {
        positions.entry(space.id).or_insert_with(|| {
            let angle = rng.r#gen::<f64>() * std::f64::consts::TAU;
            let radius = base_radius * 2.0 + rng.r#gen::<f64>() * 5.0;
            (radius * angle.cos(), radius * angle.sin())
        });
    }

    positions
}

/// Find the root space: PreferCentral, then Entry, then first.
fn find_root(spatial: &SpatialPlan) -> SpaceId {
    use crate::intent::graph::NodeRole;

    for constraint in &spatial.constraints {
        if let SpatialConstraint::PreferCentral { space } = constraint
            && spatial.spaces.iter().any(|s| s.id == *space)
        {
            return *space;
        }
    }

    spatial
        .spaces
        .iter()
        .find(|s| s.role == NodeRole::Entry)
        .or_else(|| spatial.spaces.first())
        .map(|s| s.id)
        .unwrap_or(SpaceId(0))
}

/// Build adjacency list from spatial links.
fn build_adjacency(spatial: &SpatialPlan) -> HashMap<SpaceId, Vec<SpaceId>> {
    let mut adj: HashMap<SpaceId, Vec<SpaceId>> = HashMap::new();
    for link in &spatial.links {
        adj.entry(link.from).or_default().push(link.to);
        adj.entry(link.to).or_default().push(link.from);
    }
    adj
}

/// Convert floating-point CENTER positions to integer grid positions as PlacedSpaces.
/// Positions in the simulation represent room centers; we convert to top-left corner
/// for the Rect by subtracting half the room dimensions.
fn snap_to_grid(
    spatial: &SpatialPlan,
    positions: &HashMap<SpaceId, (f64, f64)>,
    dimensions: &HashMap<SpaceId, (i32, i32)>,
) -> Vec<PlacedSpace> {
    spatial
        .spaces
        .iter()
        .map(|space| {
            let (cx, cy) = positions[&space.id];
            let (w, h) = dimensions[&space.id];
            // Convert center position to top-left corner
            let x = (cx - w as f64 / 2.0).round() as i32;
            let y = (cy - h as f64 / 2.0).round() as i32;
            let rect = Rect { x, y, w, h };
            PlacedSpace {
                space_id: space.id,
                rect,
                footprint: Footprint::Rect(rect),
                style: space.style,
                label: space.label.clone(),
            }
        })
        .collect()
}

/// Iteratively resolve overlaps by pushing rooms apart.
/// Runs multiple passes until no overlaps remain or max iterations reached.
/// Computes actual overlap/deficit depth to push by the correct amount.
fn resolve_overlaps(placed: &mut [PlacedSpace], min_gap: i32) {
    let max_iterations = 100;

    for _ in 0..max_iterations {
        let mut any_violation = false;

        for i in 0..placed.len() {
            for j in (i + 1)..placed.len() {
                let ri = placed[i].rect;
                let rj = placed[j].rect;

                // Compute per-axis overlap or gap.
                // Positive = gap, Negative = overlap (penetration depth).
                let sep_x = if ri.x + ri.w <= rj.x {
                    rj.x - (ri.x + ri.w) // gap to the right
                } else if rj.x + rj.w <= ri.x {
                    ri.x - (rj.x + rj.w) // gap to the left
                } else {
                    // Overlap in x: compute penetration depth (negative)
                    let overlap = (ri.x + ri.w).min(rj.x + rj.w) - ri.x.max(rj.x);
                    -overlap
                };

                let sep_y = if ri.y + ri.h <= rj.y {
                    rj.y - (ri.y + ri.h)
                } else if rj.y + rj.h <= ri.y {
                    ri.y - (rj.y + rj.h)
                } else {
                    let overlap = (ri.y + ri.h).min(rj.y + rj.h) - ri.y.max(rj.y);
                    -overlap
                };

                // If both axes have sufficient gap, no violation.
                // gap_to semantics: if both separated, min gap is min(sep_x, sep_y).
                // if one axis overlaps: the "gap" is the other axis's separation.
                // if both overlap: gap = 0.
                let effective_gap = if sep_x >= 0 && sep_y >= 0 {
                    sep_x.min(sep_y)
                } else {
                    sep_x.max(sep_y) // at least one is negative; max gives the less-bad axis
                };

                if effective_gap >= min_gap {
                    continue;
                }

                any_violation = true;

                let ci = ri.center();
                let cj = rj.center();
                let dx = cj.x - ci.x;
                let dy = cj.y - ci.y;

                // Choose the axis that actually needs correction.
                // A deficit ≤ 0 means that axis already meets min_gap; don't push there.
                let deficit_x = min_gap - sep_x;
                let deficit_y = min_gap - sep_y;

                let push_x = if deficit_x <= 0 && deficit_y > 0 {
                    false // only y needs work
                } else if deficit_y <= 0 && deficit_x > 0 {
                    true // only x needs work
                } else if deficit_x > 0 && deficit_y > 0 {
                    // Both need work: push on the cheaper axis (smaller deficit)
                    deficit_x <= deficit_y
                } else {
                    // Both ≤ 0 (shouldn't reach here given effective_gap < min_gap)
                    deficit_x >= deficit_y
                };

                let deficit = if push_x { deficit_x } else { deficit_y };
                // Push each room by half the deficit (with +1 to guarantee clearance)
                let shift = (deficit + 2) / 2;

                if push_x {
                    if dx >= 0 {
                        placed[j].rect.x += shift;
                        placed[i].rect.x -= shift;
                    } else {
                        placed[j].rect.x -= shift;
                        placed[i].rect.x += shift;
                    }
                } else if dy >= 0 {
                    placed[j].rect.y += shift;
                    placed[i].rect.y -= shift;
                } else {
                    placed[j].rect.y -= shift;
                    placed[i].rect.y += shift;
                }

                // Update footprints
                placed[i].footprint = Footprint::Rect(placed[i].rect);
                placed[j].footprint = Footprint::Rect(placed[j].rect);
            }
        }

        if !any_violation {
            break;
        }
    }
}

/// Apply constraint-based adjustments after overlap resolution.
/// Iterates until both separation constraints and overlap-free properties hold.
fn apply_force_constraints(
    placed: &mut [PlacedSpace],
    constraints: &ConstraintInfo,
    config: &PlacementConfig,
) {
    // Iterate: enforce separation, then resolve overlaps that might have been
    // introduced. Repeat until stable or max iterations reached.
    let max_rounds = 20;
    for _ in 0..max_rounds {
        let mut any_adjustment = false;

        // Enforce MustBeSeparated with extra gap
        for &(a, b) in &constraints.separated_pairs {
            let idx_a = placed.iter().position(|s| s.space_id == a);
            let idx_b = placed.iter().position(|s| s.space_id == b);

            if let (Some(idx_a), Some(idx_b)) = (idx_a, idx_b) {
                let gap = placed[idx_a].rect.gap_to(&placed[idx_b].rect);
                if gap < config.separation_gap {
                    any_adjustment = true;

                    // Compute axis-aligned gaps to decide which axis to push on.
                    // Push on the axis with the SMALLER gap (since gap_to = min of both).
                    let ra = placed[idx_a].rect;
                    let rb = placed[idx_b].rect;

                    let gap_x = compute_axis_gap_x(&ra, &rb);
                    let gap_y = compute_axis_gap_y(&ra, &rb);

                    // If both axes have gaps (diagonal), push on the smaller one.
                    // If only one axis has a gap, push on that axis.
                    let push_x = if gap_x > 0 && gap_y > 0 {
                        gap_x <= gap_y
                    } else if gap_x > 0 {
                        true
                    } else if gap_y > 0 {
                        false
                    } else {
                        // Overlapping — use center-to-center direction
                        let dx = rb.center().x - ra.center().x;
                        let dy = rb.center().y - ra.center().y;
                        dx.abs() >= dy.abs()
                    };

                    let deficit = config.separation_gap - gap;
                    let half_deficit = (deficit + 1) / 2;

                    if push_x {
                        let dx = rb.center().x - ra.center().x;
                        if dx >= 0 {
                            placed[idx_b].rect.x += half_deficit;
                            placed[idx_a].rect.x -= half_deficit;
                        } else {
                            placed[idx_b].rect.x -= half_deficit;
                            placed[idx_a].rect.x += half_deficit;
                        }
                    } else {
                        let dy = rb.center().y - ra.center().y;
                        if dy >= 0 {
                            placed[idx_b].rect.y += half_deficit;
                            placed[idx_a].rect.y -= half_deficit;
                        } else {
                            placed[idx_b].rect.y -= half_deficit;
                            placed[idx_a].rect.y += half_deficit;
                        }
                    }
                    placed[idx_a].footprint = Footprint::Rect(placed[idx_a].rect);
                    placed[idx_b].footprint = Footprint::Rect(placed[idx_b].rect);
                }
            }
        }

        // Resolve overlaps that separation enforcement may have introduced
        resolve_overlaps(placed, config.min_gap);

        if !any_adjustment {
            break;
        }
    }
}

/// Compute the x-axis gap between two rects. Returns 0 if they overlap in x.
fn compute_axis_gap_x(a: &Rect, b: &Rect) -> i32 {
    if a.x + a.w <= b.x {
        b.x - (a.x + a.w)
    } else if b.x + b.w <= a.x {
        a.x - (b.x + b.w)
    } else {
        0
    }
}

/// Compute the y-axis gap between two rects. Returns 0 if they overlap in y.
fn compute_axis_gap_y(a: &Rect, b: &Rect) -> i32 {
    if a.y + a.h <= b.y {
        b.y - (a.y + a.h)
    } else if b.y + b.h <= a.y {
        a.y - (b.y + b.h)
    } else {
        0
    }
}

/// Normalize all placed spaces so minimum coordinates are at a positive margin.
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

/// Apply shape refinement to each placed space (same logic as column planner).
fn apply_shape_refinement(
    placed: &mut [PlacedSpace],
    spatial: &SpatialPlan,
    location_kind: LocationKind,
    rng: &mut dyn rand::RngCore,
) {
    for space in placed.iter_mut() {
        let spec = spatial
            .spaces
            .iter()
            .find(|s| s.id == space.space_id)
            .expect("placed space must have matching spec");

        if space.rect.w < 7 || space.rect.h < 7 {
            continue;
        }

        let shape = select_shape_refinement(location_kind, spec);
        let new_footprint = shape.refine(space.rect, spec, rng);

        if new_footprint != Footprint::Rect(space.rect) {
            debug!(
                id = space.space_id.0,
                cells = match &new_footprint {
                    Footprint::Cells(c) => c.len(),
                    _ => 0,
                },
                "applied irregular shape"
            );
            space.footprint = new_footprint;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::graph::EdgeRole;
    use crate::intent::graph::{NodeRole, ScenarioNodeId};
    use crate::spatial::plan::{
        AtomicSpace, RealizationStyle, SizeHint, SpaceArchetype, SpaceKind, SpaceLink, SpaceSpec,
    };
    use crate::validate::Validator;
    use crate::validate::geometry::{GeometryValidator, GeometryValidatorConfig};
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn make_space(id: u32, role: NodeRole, w: i32, h: i32) -> SpaceSpec {
        SpaceSpec {
            id: SpaceId(id),
            origin: ScenarioNodeId(id),
            role,
            structural_tags: vec![],
            atmosphere_tags: vec![],
            motifs: vec![],
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
            location_kind: LocationKind::Dungeon,
        }
    }

    #[test]
    fn force_directed_produces_valid_plan() {
        let spatial = hub_and_entry_plan();
        let planner = ForceDirectedGeometryPlanner::default();
        let mut rng = StdRng::seed_from_u64(42);
        let plan = planner.plan(&spatial, &mut rng).unwrap();

        assert_eq!(plan.spaces.len(), 4);
        // All spaces should have positive coordinates
        for space in &plan.spaces {
            assert!(
                space.rect.x >= 0,
                "space {:?} has negative x",
                space.space_id
            );
            assert!(
                space.rect.y >= 0,
                "space {:?} has negative y",
                space.space_id
            );
        }
    }

    #[test]
    fn force_directed_no_overlaps() {
        let spatial = hub_and_entry_plan();
        let planner = ForceDirectedGeometryPlanner::default();
        let mut rng = StdRng::seed_from_u64(42);
        let plan = planner.plan(&spatial, &mut rng).unwrap();

        for i in 0..plan.spaces.len() {
            for j in (i + 1)..plan.spaces.len() {
                assert!(
                    !plan.spaces[i].rect.overlaps(&plan.spaces[j].rect),
                    "spaces {:?} and {:?} overlap",
                    plan.spaces[i].space_id,
                    plan.spaces[j].space_id,
                );
            }
        }
    }

    #[test]
    fn force_directed_minimum_gap_enforced() {
        let spatial = hub_and_entry_plan();
        let planner = ForceDirectedGeometryPlanner::default();
        let mut rng = StdRng::seed_from_u64(42);
        let plan = planner.plan(&spatial, &mut rng).unwrap();

        let min_gap = planner.config.placement.min_gap;
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
    fn force_directed_passes_geometry_validator() {
        let spatial = hub_and_entry_plan();
        let planner = ForceDirectedGeometryPlanner::default();
        let mut rng = StdRng::seed_from_u64(42);
        let plan = planner.plan(&spatial, &mut rng).unwrap();

        let validator = GeometryValidator::new(GeometryValidatorConfig { min_spacing: 1 });
        let result = validator.validate(&plan);

        let errors: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == crate::validate::Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "geometry validator should pass, got: {:?}",
            errors
        );
    }

    #[test]
    fn force_directed_must_be_separated_enforces_gap() {
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
            location_kind: LocationKind::Dungeon,
        };

        let planner = ForceDirectedGeometryPlanner {
            config: ForceDirectedConfig {
                placement: PlacementConfig {
                    min_gap: 2,
                    separation_gap: 8,
                },
                ..Default::default()
            },
        };
        let mut rng = StdRng::seed_from_u64(42);
        let plan = planner.plan(&spatial, &mut rng).unwrap();

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
            "MustBeSeparated spaces should have at least separation_gap=8, got {}",
            gap
        );
    }

    #[test]
    fn force_directed_empty_plan_returns_error() {
        let spatial = SpatialPlan {
            spaces: vec![],
            links: vec![],
            constraints: vec![],
            location_kind: LocationKind::Dungeon,
        };
        let planner = ForceDirectedGeometryPlanner::default();
        let mut rng = StdRng::seed_from_u64(42);
        assert!(planner.plan(&spatial, &mut rng).is_err());
    }

    #[test]
    fn force_directed_single_space() {
        let spatial = SpatialPlan {
            spaces: vec![make_space(0, NodeRole::Entry, 7, 7)],
            links: vec![],
            constraints: vec![],
            location_kind: LocationKind::Dungeon,
        };
        let planner = ForceDirectedGeometryPlanner::default();
        let mut rng = StdRng::seed_from_u64(42);
        let plan = planner.plan(&spatial, &mut rng).unwrap();

        assert_eq!(plan.spaces.len(), 1);
        assert_eq!(plan.spaces[0].rect.w, 7);
        assert_eq!(plan.spaces[0].rect.h, 7);
    }

    #[test]
    fn force_directed_deterministic_with_same_seed() {
        let spatial = hub_and_entry_plan();
        let planner = ForceDirectedGeometryPlanner::default();

        let mut rng1 = StdRng::seed_from_u64(123);
        let plan1 = planner.plan(&spatial, &mut rng1).unwrap();

        let mut rng2 = StdRng::seed_from_u64(123);
        let plan2 = planner.plan(&spatial, &mut rng2).unwrap();

        for (s1, s2) in plan1.spaces.iter().zip(plan2.spaces.iter()) {
            assert_eq!(
                s1.rect, s2.rect,
                "plans should be deterministic with same seed"
            );
        }
    }

    #[test]
    fn force_directed_medium_scale_plan() {
        // 10-room graph to test scaling behavior
        let spaces: Vec<SpaceSpec> = (0..10)
            .map(|i| {
                let role = match i {
                    0 => NodeRole::Entry,
                    5 => NodeRole::Hub,
                    9 => NodeRole::Goal,
                    _ => NodeRole::Branch,
                };
                make_space(i, role, 7, 7)
            })
            .collect();

        // Linear chain with some branches
        let links = vec![
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
                from: SpaceId(2),
                to: SpaceId(5),
                role: EdgeRole::Traversal,
                tags: vec![],
            },
            SpaceLink {
                from: SpaceId(5),
                to: SpaceId(3),
                role: EdgeRole::Traversal,
                tags: vec![],
            },
            SpaceLink {
                from: SpaceId(5),
                to: SpaceId(4),
                role: EdgeRole::Traversal,
                tags: vec![],
            },
            SpaceLink {
                from: SpaceId(5),
                to: SpaceId(6),
                role: EdgeRole::Traversal,
                tags: vec![],
            },
            SpaceLink {
                from: SpaceId(6),
                to: SpaceId(7),
                role: EdgeRole::Traversal,
                tags: vec![],
            },
            SpaceLink {
                from: SpaceId(7),
                to: SpaceId(8),
                role: EdgeRole::Traversal,
                tags: vec![],
            },
            SpaceLink {
                from: SpaceId(8),
                to: SpaceId(9),
                role: EdgeRole::Traversal,
                tags: vec![],
            },
        ];

        let spatial = SpatialPlan {
            spaces,
            links,
            constraints: vec![SpatialConstraint::PreferCentral { space: SpaceId(5) }],
            location_kind: LocationKind::Dungeon,
        };

        let planner = ForceDirectedGeometryPlanner::default();
        let mut rng = StdRng::seed_from_u64(42);
        let plan = planner.plan(&spatial, &mut rng).unwrap();

        assert_eq!(plan.spaces.len(), 10);

        // No overlaps
        for i in 0..plan.spaces.len() {
            for j in (i + 1)..plan.spaces.len() {
                assert!(
                    !plan.spaces[i].rect.overlaps(&plan.spaces[j].rect),
                    "spaces {:?} and {:?} overlap in medium-scale plan",
                    plan.spaces[i].space_id,
                    plan.spaces[j].space_id,
                );
            }
        }

        // Validator passes
        let validator = GeometryValidator::new(GeometryValidatorConfig { min_spacing: 1 });
        let result = validator.validate(&plan);
        let errors: Vec<_> = result
            .issues
            .iter()
            .filter(|i| i.severity == crate::validate::Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "medium-scale plan should pass validation, got: {:?}",
            errors
        );
    }
}
