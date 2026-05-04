//! Shared helpers for geometry planners.

use std::collections::HashMap;

use tracing::debug;

use crate::geometry::geom::*;
use crate::geometry::shape::select_shape_refinement;
use crate::intent::graph::NodeRole;
use crate::intent::map_intent::LocationKind;
use crate::spatial::plan::{SpaceId, SpaceKind, SpatialConstraint, SpatialPlan};

/// Extract width/height from a space's kind.
pub fn get_space_dimensions(space: &crate::spatial::plan::SpaceSpec) -> (i32, i32) {
    match &space.kind {
        SpaceKind::Atomic(a) => (a.width, a.height),
    }
}

/// Build adjacency list from spatial links.
pub fn build_adjacency(spatial: &SpatialPlan) -> HashMap<SpaceId, Vec<SpaceId>> {
    let mut adj: HashMap<SpaceId, Vec<SpaceId>> = HashMap::new();
    for link in &spatial.links {
        adj.entry(link.from).or_default().push(link.to);
        adj.entry(link.to).or_default().push(link.from);
    }
    adj
}

/// Find the BFS root space: PreferCentral, then Entry, then first.
pub fn find_bfs_root(spatial: &SpatialPlan) -> SpaceId {
    // Check for PreferCentral constraint — use that space as root.
    for constraint in &spatial.constraints {
        if let SpatialConstraint::PreferCentral { space } = constraint {
            if spatial.spaces.iter().any(|s| s.id == *space) {
                return *space;
            }
        }
    }

    // Fall back to the entry space, then the first space.
    spatial
        .spaces
        .iter()
        .find(|s| s.role == NodeRole::Entry)
        .or_else(|| spatial.spaces.first())
        .map(|s| s.id)
        .unwrap_or(SpaceId(0))
}

/// Normalize all placed spaces so minimum coordinates are at a positive margin.
pub fn normalize_positions(placed: &mut [PlacedSpace]) {
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

/// Apply shape refinement to each placed space based on location kind and tags.
///
/// Rooms with `LocationKind::Cave` or atmosphere tag `"natural"` (and large enough
/// bounding rect >= 7x7) get organic irregular footprints via `CaveIrregularizer`.
/// All other rooms keep their rectangular footprints.
pub fn apply_shape_refinement(
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
