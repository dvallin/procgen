//! Street-skeleton geometry planner.
//!
//! Specialized planner for urban layouts:
//! 1. Places a "backbone" of streets, alleys, and plazas as axis-aligned rectangles.
//! 2. Attaches buildings (shops, warehouses, chambers) flush against backbone edges.
//! 3. Resolves overlaps, normalizes, applies shape refinement, and routes corridors.
//!
//! Falls back to a simple column-like placement when no backbone spaces exist.

use std::collections::{HashMap, HashSet, VecDeque};

use tracing::{debug, info, info_span, warn};

use crate::geometry::common::{
    apply_shape_refinement, build_adjacency, get_space_dimensions, normalize_positions,
};
use crate::geometry::geom::*;
use crate::geometry::planner::{GeometryPlanError, GeometryPlanner, PlacementConfig};
use crate::geometry::routing::{CorridorRouter, ZShapeRouter};
use crate::spatial::plan::{
    AlignSide, Axis, SpaceArchetype, SpaceId, SpatialConstraint, SpatialPlan,
};

// ---------------------------------------------------------------------------
// Planner
// ---------------------------------------------------------------------------

/// A geometry planner that lays out urban environments using a street skeleton.
///
/// Backbone spaces (streets, alleys, plazas) are placed first along their
/// preferred axes, then buildings are attached to the edges of the backbone.
#[derive(Default)]
pub struct StreetSkeletonPlanner {
    pub config: PlacementConfig,
}

impl GeometryPlanner for StreetSkeletonPlanner {
    fn plan(
        &self,
        spatial: &SpatialPlan,
        rng: &mut dyn rand::RngCore,
    ) -> Result<GeometryPlan, GeometryPlanError> {
        if spatial.spaces.is_empty() {
            return Err(GeometryPlanError::EmptyPlan);
        }

        let _span = info_span!(
            "street_skeleton_planning",
            spaces = spatial.spaces.len(),
            links = spatial.links.len()
        )
        .entered();

        let adjacency = build_adjacency(spatial);

        // ── 1. Partition spaces into backbone vs attached ──
        let (backbone_ids, attached_ids) = partition_spaces(spatial);
        debug!(
            backbone = backbone_ids.len(),
            attached = attached_ids.len(),
            "partitioned spaces"
        );

        // ── 2. Place all spaces ──
        let mut placed = if backbone_ids.is_empty() {
            // Fallback: no backbone, place all spaces in a simple column layout.
            warn!("no backbone spaces found, falling back to column placement");
            fallback_column_placement(spatial, &self.config)
        } else {
            // Normal path: place backbone first, then attach buildings.
            let mut placed = place_backbone(spatial, &backbone_ids, &adjacency, &self.config);
            attach_buildings(
                spatial,
                &attached_ids,
                &mut placed,
                &adjacency,
                &self.config,
            );
            placed
        };

        // ── 3. Resolve overlaps ──
        resolve_overlaps(&mut placed, self.config.min_gap);

        // ── 4. Normalize positions ──
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

        // ── 5. Shape refinement ──
        let location_kind = spatial.location_kind;
        apply_shape_refinement(&mut placed, spatial, location_kind, rng);

        // ── 6. Route corridors ──
        let links = ZShapeRouter.route(spatial, &placed);
        info!(corridors = links.len(), "routing complete");

        Ok(GeometryPlan {
            spaces: placed,
            links,
        })
    }
}

// ---------------------------------------------------------------------------
// Partitioning
// ---------------------------------------------------------------------------

/// Returns true if the space archetype belongs to the backbone (street network).
fn is_backbone_archetype(archetype: Option<SpaceArchetype>) -> bool {
    matches!(
        archetype,
        Some(SpaceArchetype::Street) | Some(SpaceArchetype::Alley) | Some(SpaceArchetype::Plaza)
    )
}

/// Partition spaces into backbone (streets/alleys/plazas) and attached (buildings).
fn partition_spaces(spatial: &SpatialPlan) -> (Vec<SpaceId>, Vec<SpaceId>) {
    let mut backbone = Vec::new();
    let mut attached = Vec::new();

    for space in &spatial.spaces {
        if is_backbone_archetype(space.archetype) {
            backbone.push(space.id);
        } else {
            attached.push(space.id);
        }
    }

    (backbone, attached)
}

// ---------------------------------------------------------------------------
// Backbone placement
// ---------------------------------------------------------------------------

/// Place backbone spaces using BFS from the best starting node.
/// Prefers plazas as starting points; otherwise uses the first backbone space.
fn place_backbone(
    spatial: &SpatialPlan,
    backbone_ids: &[SpaceId],
    adjacency: &HashMap<SpaceId, Vec<SpaceId>>,
    config: &PlacementConfig,
) -> Vec<PlacedSpace> {
    let backbone_set: HashSet<SpaceId> = backbone_ids.iter().copied().collect();

    // Pick a starting node: prefer a Plaza, then first backbone space.
    let start_id = backbone_ids
        .iter()
        .find(|id| {
            spatial
                .spaces
                .iter()
                .find(|s| s.id == **id)
                .and_then(|s| s.archetype)
                == Some(SpaceArchetype::Plaza)
        })
        .copied()
        .unwrap_or(backbone_ids[0]);

    let mut placed: Vec<PlacedSpace> = Vec::new();
    let mut placed_set: HashSet<SpaceId> = HashSet::new();
    let mut queue: VecDeque<SpaceId> = VecDeque::new();

    // Place the starting space at origin.
    let start_spec = spatial.spaces.iter().find(|s| s.id == start_id).unwrap();
    let (w, h) = get_space_dimensions(start_spec);
    placed.push(make_placed_space(start_spec, Rect { x: 0, y: 0, w, h }));
    placed_set.insert(start_id);
    queue.push_back(start_id);

    // Track which side of each space we've used, so we cycle through directions.
    // Use per-axis counters: (space_id, axis) → how many times used on that axis.
    let mut axis_counter: HashMap<(SpaceId, Option<Axis>), u32> = HashMap::new();

    // BFS along backbone links.
    while let Some(current_id) = queue.pop_front() {
        let neighbors = adjacency.get(&current_id).cloned().unwrap_or_default();
        for neighbor_id in neighbors {
            if placed_set.contains(&neighbor_id) || !backbone_set.contains(&neighbor_id) {
                continue;
            }

            let neighbor_spec = match spatial.spaces.iter().find(|s| s.id == neighbor_id) {
                Some(s) => s,
                None => continue,
            };
            let (nw, nh) = get_space_dimensions(neighbor_spec);

            // Determine placement axis from PreferOrientation constraint.
            let preferred_axis = get_preferred_axis(spatial, neighbor_id);

            // Find the rect of the current (parent) space.
            let parent_rect = placed
                .iter()
                .find(|p| p.space_id == current_id)
                .map(|p| p.rect)
                .unwrap();

            // Count how many children with this axis we've already placed from this parent.
            let axis_idx = axis_counter
                .entry((current_id, preferred_axis))
                .or_insert(0);
            let direction = *axis_idx;
            *axis_idx += 1;

            // Place neighbor along the appropriate direction relative to parent.
            let gap = 1; // Tight gap for connected backbone elements.
            let rect = compute_backbone_position_distributed(
                parent_rect,
                nw,
                nh,
                preferred_axis,
                direction,
                gap,
            );

            placed.push(make_placed_space(neighbor_spec, rect));
            placed_set.insert(neighbor_id);
            queue.push_back(neighbor_id);
        }
    }

    // Handle backbone spaces that weren't reached by BFS (disconnected).
    for &id in backbone_ids {
        if placed_set.contains(&id) {
            continue;
        }
        let spec = spatial.spaces.iter().find(|s| s.id == id).unwrap();
        let (w, h) = get_space_dimensions(spec);
        let offset_y = placed
            .iter()
            .map(|p| p.rect.y + p.rect.h)
            .max()
            .unwrap_or(0)
            + config.min_gap;
        placed.push(make_placed_space(
            spec,
            Rect {
                x: 0,
                y: offset_y,
                w,
                h,
            },
        ));
        placed_set.insert(id);
    }

    placed
}

/// Compute where to place a backbone neighbor relative to its parent,
/// distributing around different sides.
///
/// `direction` cycles 0=East, 1=South, 2=West, 3=North.
/// `preferred_axis` can override to ensure the street is on the correct axis
/// (e.g. horizontal street goes east/west, vertical street goes north/south).
fn compute_backbone_position_distributed(
    parent: Rect,
    w: i32,
    h: i32,
    preferred_axis: Option<Axis>,
    direction: u32,
    gap: i32,
) -> Rect {
    // If the child has a preferred axis, pick a compatible direction:
    // Horizontal → East(0) or West(2); Vertical → South(1) or North(3).
    let effective_dir = match preferred_axis {
        Some(Axis::Horizontal) => {
            // Alternate: E, W, E, W, ...
            if direction % 2 == 0 { 0 } else { 2 }
        }
        Some(Axis::Vertical) => {
            // Alternate: S, N, S, N, ...
            if direction % 2 == 0 { 1 } else { 3 }
        }
        None => direction % 4,
    };

    match effective_dir {
        0 => {
            // East: place to the right, centered vertically.
            let x = parent.x + parent.w + gap;
            let y = parent.y + (parent.h - h) / 2;
            Rect { x, y, w, h }
        }
        1 => {
            // South: place below, centered horizontally.
            let x = parent.x + (parent.w - w) / 2;
            let y = parent.y + parent.h + gap;
            Rect { x, y, w, h }
        }
        2 => {
            // West: place to the left, centered vertically.
            let x = parent.x - w - gap;
            let y = parent.y + (parent.h - h) / 2;
            Rect { x, y, w, h }
        }
        3 | _ => {
            // North: place above, centered horizontally.
            let x = parent.x + (parent.w - w) / 2;
            let y = parent.y - h - gap;
            Rect { x, y, w, h }
        }
    }
}

/// Look up a PreferOrientation constraint for the given space.
fn get_preferred_axis(spatial: &SpatialPlan, space_id: SpaceId) -> Option<Axis> {
    spatial.constraints.iter().find_map(|c| match c {
        SpatialConstraint::PreferOrientation { space, axis } if *space == space_id => Some(*axis),
        _ => None,
    })
}

// ---------------------------------------------------------------------------
// Building attachment
// ---------------------------------------------------------------------------

/// Attach building spaces to the backbone edges they are linked to.
fn attach_buildings(
    spatial: &SpatialPlan,
    attached_ids: &[SpaceId],
    placed: &mut Vec<PlacedSpace>,
    adjacency: &HashMap<SpaceId, Vec<SpaceId>>,
    config: &PlacementConfig,
) {
    let mut placed_set: HashSet<SpaceId> = placed.iter().map(|p| p.space_id).collect();

    // Track how many buildings are already attached on each side of each backbone space.
    let mut side_counts: HashMap<(SpaceId, AlignSide), i32> = HashMap::new();

    for &building_id in attached_ids {
        let spec = match spatial.spaces.iter().find(|s| s.id == building_id) {
            Some(s) => s,
            None => continue,
        };
        let (bw, bh) = get_space_dimensions(spec);

        // Find a backbone neighbor to attach to.
        let linked_backbone = adjacency
            .get(&building_id)
            .and_then(|neighbors| neighbors.iter().find(|n| placed_set.contains(n)))
            .copied();

        if let Some(street_id) = linked_backbone {
            let street_rect = placed
                .iter()
                .find(|p| p.space_id == street_id)
                .map(|p| p.rect)
                .unwrap();

            // Determine which side to attach on, avoiding overlaps.
            let side = determine_attach_side_no_overlap(
                spatial,
                building_id,
                street_id,
                street_rect,
                bw,
                bh,
                &side_counts,
                placed,
                config.min_gap,
            );
            let count = side_counts.entry((street_id, side)).or_insert(0);

            let rect = compute_building_position(street_rect, bw, bh, side, *count, config.min_gap);
            *count += 1;

            debug!(
                building = building_id.0,
                street = street_id.0,
                ?side,
                "attached building"
            );
            placed.push(make_placed_space(spec, rect));
            placed_set.insert(building_id);
        } else {
            // No backbone neighbor found — place below existing spaces.
            let offset_y = placed
                .iter()
                .map(|p| p.rect.y + p.rect.h)
                .max()
                .unwrap_or(0)
                + config.min_gap;
            debug!(
                building = building_id.0,
                "no backbone link, placing at bottom"
            );
            placed.push(make_placed_space(
                spec,
                Rect {
                    x: 0,
                    y: offset_y,
                    w: bw,
                    h: bh,
                },
            ));
            placed_set.insert(building_id);
        }
    }
}

/// Determine which side of the street to attach the building.
/// Checks `AttachToEdge` and `AlignEdge` constraints; falls back to least-used side
/// that does not cause overlaps with already-placed spaces.
#[allow(clippy::too_many_arguments)]
fn determine_attach_side_no_overlap(
    spatial: &SpatialPlan,
    building_id: SpaceId,
    street_id: SpaceId,
    street_rect: Rect,
    bw: i32,
    bh: i32,
    side_counts: &HashMap<(SpaceId, AlignSide), i32>,
    placed: &[PlacedSpace],
    min_gap: i32,
) -> AlignSide {
    // Check for explicit constraint first.
    for constraint in &spatial.constraints {
        match constraint {
            SpatialConstraint::AttachToEdge {
                building,
                street,
                side,
            } if *building == building_id && *street == street_id => {
                return *side;
            }
            SpatialConstraint::AlignEdge { a, b, side }
                if (*a == street_id && *b == building_id)
                    || (*a == building_id && *b == street_id) =>
            {
                return *side;
            }
            _ => {}
        }
    }

    // Fallback: pick the side with fewest existing attachments that doesn't overlap.
    let candidates = [
        AlignSide::North,
        AlignSide::South,
        AlignSide::East,
        AlignSide::West,
    ];

    // Score each side: (overlaps_count, existing_count)
    let mut best_side = AlignSide::North;
    let mut best_score = (i32::MAX, i32::MAX);

    for &side in &candidates {
        let count = side_counts.get(&(street_id, side)).copied().unwrap_or(0);
        let candidate_rect = compute_building_position(street_rect, bw, bh, side, count, min_gap);

        // Count how many existing spaces this candidate would overlap with.
        let overlap_count = placed
            .iter()
            .filter(|p| p.rect.overlaps(&candidate_rect))
            .count() as i32;

        let score = (overlap_count, count);
        if score < best_score {
            best_score = score;
            best_side = side;
        }
    }

    best_side
}

/// Compute the position of a building attached to a specific side of a street.
fn compute_building_position(
    street_rect: Rect,
    bw: i32,
    bh: i32,
    side: AlignSide,
    offset_index: i32,
    gap: i32,
) -> Rect {
    // Offset along the street edge so multiple buildings don't stack on the same spot.
    let lateral_offset = offset_index * (bw + gap);

    match side {
        AlignSide::North => {
            // Place building above the street.
            let x = street_rect.x + lateral_offset;
            let y = street_rect.y - bh - gap;
            Rect { x, y, w: bw, h: bh }
        }
        AlignSide::South => {
            // Place building below the street.
            let x = street_rect.x + lateral_offset;
            let y = street_rect.y + street_rect.h + gap;
            Rect { x, y, w: bw, h: bh }
        }
        AlignSide::East => {
            // Place building to the east of the street.
            let x = street_rect.x + street_rect.w + gap;
            let y = street_rect.y + lateral_offset;
            Rect { x, y, w: bw, h: bh }
        }
        AlignSide::West => {
            // Place building to the west of the street.
            let x = street_rect.x - bw - gap;
            let y = street_rect.y + lateral_offset;
            Rect { x, y, w: bw, h: bh }
        }
    }
}

// ---------------------------------------------------------------------------
// Fallback placement (no backbone)
// ---------------------------------------------------------------------------

/// Simple column-like fallback when no backbone spaces exist.
/// Places spaces vertically with min_gap between them.
fn fallback_column_placement(spatial: &SpatialPlan, config: &PlacementConfig) -> Vec<PlacedSpace> {
    let mut placed = Vec::new();
    let mut y_cursor = 0;

    for spec in &spatial.spaces {
        let (w, h) = get_space_dimensions(spec);
        let rect = Rect {
            x: 0,
            y: y_cursor,
            w,
            h,
        };
        placed.push(make_placed_space(spec, rect));
        y_cursor += h + config.min_gap;
    }

    placed
}

// ---------------------------------------------------------------------------
// Overlap resolution
// ---------------------------------------------------------------------------

/// Iteratively push overlapping spaces apart until no overlaps remain.
fn resolve_overlaps(placed: &mut [PlacedSpace], min_gap: i32) {
    const MAX_ITERATIONS: usize = 200;

    for iteration in 0..MAX_ITERATIONS {
        let mut any_overlap = false;

        for i in 0..placed.len() {
            for j in (i + 1)..placed.len() {
                let half_gap = min_gap / 2;
                let inflated_a = placed[i].rect.inflate(half_gap);
                let inflated_b = placed[j].rect.inflate(half_gap);

                if inflated_a.overlaps(&inflated_b) {
                    any_overlap = true;

                    // Compute overlap extent on each axis.
                    let overlap_x = (inflated_a.x + inflated_a.w).min(inflated_b.x + inflated_b.w)
                        - inflated_a.x.max(inflated_b.x);
                    let overlap_y = (inflated_a.y + inflated_a.h).min(inflated_b.y + inflated_b.h)
                        - inflated_a.y.max(inflated_b.y);

                    let center_a = placed[i].rect.center();
                    let center_b = placed[j].rect.center();
                    let dx = center_b.x - center_a.x;
                    let dy = center_b.y - center_a.y;

                    // Push along the axis of least overlap, by half the overlap amount (at least 1).
                    if overlap_x <= overlap_y {
                        let push = (overlap_x / 2).max(1) * if dx >= 0 { 1 } else { -1 };
                        placed[j].rect.x += push;
                    } else {
                        let push = (overlap_y / 2).max(1) * if dy >= 0 { 1 } else { -1 };
                        placed[j].rect.y += push;
                    }
                    // Update footprint.
                    placed[j].footprint = Footprint::Rect(placed[j].rect);
                }
            }
        }

        if !any_overlap {
            debug!(iterations = iteration + 1, "overlap resolution converged");
            return;
        }
    }

    warn!(
        iterations = MAX_ITERATIONS,
        "overlap resolution did not fully converge"
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a `PlacedSpace` from a spec and rect.
fn make_placed_space(spec: &crate::spatial::plan::SpaceSpec, rect: Rect) -> PlacedSpace {
    PlacedSpace {
        space_id: spec.id,
        rect,
        footprint: Footprint::Rect(rect),
        style: spec.style,
        label: spec.label.clone(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::graph::{EdgeRole, NodeRole, ScenarioNodeId};
    use crate::intent::map_intent::LocationKind;
    use crate::spatial::plan::{
        AtomicSpace, RealizationStyle, SizeHint, SpaceKind, SpaceLink, SpaceSpec, SpatialPlan,
    };
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    /// Helper: build a minimal SpaceSpec with the given archetype and dimensions.
    fn make_space(
        id: u32,
        archetype: Option<SpaceArchetype>,
        role: NodeRole,
        width: i32,
        height: i32,
    ) -> SpaceSpec {
        SpaceSpec {
            id: SpaceId(id),
            origin: ScenarioNodeId(id),
            role,
            structural_tags: vec![],
            atmosphere_tags: vec![],
            motifs: vec![],
            style: RealizationStyle::RoomLike,
            kind: SpaceKind::Atomic(AtomicSpace { width, height }),
            label: Some(format!("space_{}", id)),
            archetype,
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

    fn make_rng() -> StdRng {
        StdRng::seed_from_u64(42)
    }

    /// Verify no two placed spaces overlap (exclusive boundary check).
    fn assert_no_overlaps(placed: &[PlacedSpace]) {
        for i in 0..placed.len() {
            for j in (i + 1)..placed.len() {
                assert!(
                    !placed[i].rect.overlaps(&placed[j].rect),
                    "Overlap between space {} ({:?}) and space {} ({:?})",
                    placed[i].space_id.0,
                    placed[i].rect,
                    placed[j].space_id.0,
                    placed[j].rect,
                );
            }
        }
    }

    // ── Test: street with shops on both sides ──

    #[test]
    fn street_skeleton_simple_street_with_shops() {
        let spatial = SpatialPlan {
            spaces: vec![
                make_space(0, Some(SpaceArchetype::Street), NodeRole::Hub, 15, 5),
                make_space(1, Some(SpaceArchetype::Shop), NodeRole::Reward, 5, 5),
                make_space(2, Some(SpaceArchetype::Shop), NodeRole::Reward, 5, 5),
            ],
            links: vec![make_link(0, 1), make_link(0, 2)],
            constraints: vec![
                SpatialConstraint::PreferOrientation {
                    space: SpaceId(0),
                    axis: Axis::Horizontal,
                },
                SpatialConstraint::AttachToEdge {
                    building: SpaceId(1),
                    street: SpaceId(0),
                    side: AlignSide::North,
                },
                SpatialConstraint::AttachToEdge {
                    building: SpaceId(2),
                    street: SpaceId(0),
                    side: AlignSide::South,
                },
            ],
            location_kind: LocationKind::Urban,
        };

        let planner = StreetSkeletonPlanner::default();
        let mut rng = make_rng();
        let result = planner.plan(&spatial, &mut rng);

        assert!(result.is_ok(), "planning should succeed");
        let plan = result.unwrap();

        assert_eq!(plan.spaces.len(), 3);
        assert_no_overlaps(&plan.spaces);

        // Verify the street is present and the shops are on opposite sides.
        let street = plan
            .spaces
            .iter()
            .find(|s| s.space_id == SpaceId(0))
            .unwrap();
        let shop_n = plan
            .spaces
            .iter()
            .find(|s| s.space_id == SpaceId(1))
            .unwrap();
        let shop_s = plan
            .spaces
            .iter()
            .find(|s| s.space_id == SpaceId(2))
            .unwrap();

        // North shop should be above street (lower y).
        assert!(
            shop_n.rect.y + shop_n.rect.h <= street.rect.y,
            "north shop should be above the street"
        );
        // South shop should be below street (higher y).
        assert!(
            shop_s.rect.y >= street.rect.y + street.rect.h,
            "south shop should be below the street"
        );
    }

    // ── Test: plaza junction ──

    #[test]
    fn street_skeleton_plaza_junction() {
        let spatial = SpatialPlan {
            spaces: vec![
                make_space(0, Some(SpaceArchetype::Plaza), NodeRole::Hub, 9, 9),
                make_space(1, Some(SpaceArchetype::Street), NodeRole::Transition, 12, 4),
                make_space(2, Some(SpaceArchetype::Street), NodeRole::Transition, 4, 12),
                make_space(3, Some(SpaceArchetype::Shop), NodeRole::Reward, 5, 5),
                make_space(4, Some(SpaceArchetype::Shop), NodeRole::Reward, 5, 5),
            ],
            links: vec![
                make_link(0, 1),
                make_link(0, 2),
                make_link(1, 3),
                make_link(2, 4),
            ],
            constraints: vec![
                SpatialConstraint::PreferOrientation {
                    space: SpaceId(1),
                    axis: Axis::Horizontal,
                },
                SpatialConstraint::PreferOrientation {
                    space: SpaceId(2),
                    axis: Axis::Vertical,
                },
            ],
            location_kind: LocationKind::Urban,
        };

        let planner = StreetSkeletonPlanner::default();
        let mut rng = make_rng();
        let result = planner.plan(&spatial, &mut rng);

        assert!(result.is_ok(), "planning should succeed");
        let plan = result.unwrap();

        assert_eq!(plan.spaces.len(), 5);
        assert_no_overlaps(&plan.spaces);

        // Verify the horizontal street is to the east (higher x) of the plaza.
        let plaza = plan
            .spaces
            .iter()
            .find(|s| s.space_id == SpaceId(0))
            .unwrap();
        let h_street = plan
            .spaces
            .iter()
            .find(|s| s.space_id == SpaceId(1))
            .unwrap();
        let v_street = plan
            .spaces
            .iter()
            .find(|s| s.space_id == SpaceId(2))
            .unwrap();

        assert!(
            h_street.rect.x >= plaza.rect.x + plaza.rect.w,
            "horizontal street should be to the east of plaza"
        );
        assert!(
            v_street.rect.y >= plaza.rect.y + plaza.rect.h,
            "vertical street should be below the plaza"
        );
    }

    // ── Test: no backbone fallback ──

    #[test]
    fn street_skeleton_no_backbone_fallback() {
        let spatial = SpatialPlan {
            spaces: vec![
                make_space(0, Some(SpaceArchetype::Chamber), NodeRole::Entry, 7, 7),
                make_space(1, Some(SpaceArchetype::Chamber), NodeRole::Hub, 7, 7),
                make_space(2, Some(SpaceArchetype::Vault), NodeRole::Goal, 6, 6),
            ],
            links: vec![make_link(0, 1), make_link(1, 2)],
            constraints: vec![],
            location_kind: LocationKind::Dungeon,
        };

        let planner = StreetSkeletonPlanner::default();
        let mut rng = make_rng();
        let result = planner.plan(&spatial, &mut rng);

        assert!(
            result.is_ok(),
            "planning should succeed even without backbone"
        );
        let plan = result.unwrap();

        assert_eq!(plan.spaces.len(), 3);
        assert_no_overlaps(&plan.spaces);

        // Verify column arrangement: each space should be below the previous.
        let s0 = plan
            .spaces
            .iter()
            .find(|s| s.space_id == SpaceId(0))
            .unwrap();
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

        assert!(s1.rect.y > s0.rect.y, "space 1 below space 0");
        assert!(s2.rect.y > s1.rect.y, "space 2 below space 1");
    }

    // ── Test: single space ──

    #[test]
    fn street_skeleton_single_space() {
        let spatial = SpatialPlan {
            spaces: vec![make_space(
                0,
                Some(SpaceArchetype::Plaza),
                NodeRole::Hub,
                9,
                9,
            )],
            links: vec![],
            constraints: vec![],
            location_kind: LocationKind::Urban,
        };

        let planner = StreetSkeletonPlanner::default();
        let mut rng = make_rng();
        let result = planner.plan(&spatial, &mut rng);

        assert!(result.is_ok(), "single space should produce valid output");
        let plan = result.unwrap();

        assert_eq!(plan.spaces.len(), 1);
        assert_eq!(plan.spaces[0].rect.w, 9);
        assert_eq!(plan.spaces[0].rect.h, 9);
    }

    // ── Test: empty plan returns error ──

    #[test]
    fn street_skeleton_empty_plan_returns_error() {
        let spatial = SpatialPlan {
            spaces: vec![],
            links: vec![],
            constraints: vec![],
            location_kind: LocationKind::Urban,
        };

        let planner = StreetSkeletonPlanner::default();
        let mut rng = make_rng();
        let result = planner.plan(&spatial, &mut rng);

        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), GeometryPlanError::EmptyPlan),
            "should return EmptyPlan error"
        );
    }
}
