//! Interior plan builder — orchestrates zone classification and path reservation.
//!
//! Produces one [`InteriorPlan`] per room by:
//! 1. Detecting doors on the room boundary.
//! 2. Computing reserved door-to-door paths via BFS.
//! 3. Reserving corridor cells that pass through the room interior.
//! 4. Classifying interior cells into zones.

use std::collections::HashMap;

use tracing::{debug, info, info_span};

use crate::geometry::geom::GeometryPlan;
use crate::spatial::plan::{SpaceId, SpatialPlan};
use crate::tile::map::TileMap;

use super::paths::{compute_corridor_passthrough, compute_reserved_paths, find_room_doors};
use super::plan::InteriorPlan;
use super::zones::classify_zones;

/// Build interior plans for all rooms in a geometry plan.
///
/// This is the main entry point for the interior planning stage.
/// Call after rasterization and scatter (needs final tile state).
///
/// When `spatial` is provided, the archetype of each space is looked up and
/// passed to zone classification for urban-aware edge zones.
pub fn build_interior_plans(
    geometry: &GeometryPlan,
    tiles: &TileMap,
    spatial: Option<&SpatialPlan>,
) -> Vec<InteriorPlan> {
    let _span = info_span!("interior_planning", rooms = geometry.spaces.len()).entered();

    let mut plans = Vec::with_capacity(geometry.spaces.len());

    for placed in &geometry.spaces {
        let rect = placed.rect;

        // Step 1: find doors on this room's boundary.
        let doors = find_room_doors(tiles, rect);

        // Step 2: compute reserved paths between doors.
        let (path_intents, mut reserved_paths) = compute_reserved_paths(tiles, rect, &doors);

        // Step 3: reserve corridor cells that pass through this room's interior.
        // Corridors routed through a room (not entering via a door) must stay clear
        // to maintain global connectivity.
        let corridor_passthrough = compute_corridor_passthrough(rect, &geometry.links, &doors);
        if !corridor_passthrough.is_empty() {
            debug!(
                space_id = placed.space_id.0,
                corridor_reserved = corridor_passthrough.len(),
                "reserving corridor pass-through cells"
            );
            reserved_paths.extend(corridor_passthrough);
        }

        // Step 4: classify zones.
        let archetype = spatial.and_then(|sp| {
            sp.spaces
                .iter()
                .find(|s| s.id == placed.space_id)
                .and_then(|s| s.archetype)
        });
        let zones = classify_zones(tiles, rect, &reserved_paths, archetype);

        debug!(
            space_id = placed.space_id.0,
            label = ?placed.label,
            doors = doors.len(),
            reserved_cells = reserved_paths.len(),
            zones = zones.len(),
            "interior plan built"
        );

        plans.push(InteriorPlan {
            space_id: placed.space_id,
            rect,
            zones,
            doors,
            reserved_paths,
            path_intents,
        });
    }

    info!(rooms = plans.len(), "interior planning complete");

    plans
}

/// Build a lookup map from SpaceId to InteriorPlan.
///
/// Useful for the feature planner which needs to look up plans by room.
pub fn interior_plan_map(plans: &[InteriorPlan]) -> HashMap<SpaceId, &InteriorPlan> {
    plans.iter().map(|p| (p.space_id, p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::geom::{Footprint, PlacedSpace, Rect};
    use crate::spatial::plan::{RealizationStyle, SpaceId};
    use crate::tile::registry::Tile;

    fn make_test_map_and_geometry() -> (TileMap, GeometryPlan) {
        // 9x9 map with a single 7x7 room at (1,1).
        let mut map = TileMap::new(9, 9);
        let rect = Rect {
            x: 1,
            y: 1,
            w: 7,
            h: 7,
        };

        // Walls
        for x in 1..8 {
            map.set(x, 1, Tile::WALL);
            map.set(x, 7, Tile::WALL);
        }
        for y in 1..8 {
            map.set(1, y, Tile::WALL);
            map.set(7, y, Tile::WALL);
        }

        // Floor
        for y in 2..7 {
            for x in 2..7 {
                map.set(x, y, Tile::FLOOR);
            }
        }

        // Doors
        map.set(1, 4, Tile::DOOR);
        map.set(7, 4, Tile::DOOR);

        let geometry = GeometryPlan {
            spaces: vec![PlacedSpace {
                space_id: SpaceId(0),
                rect,
                footprint: Footprint::Rect(rect),
                style: RealizationStyle::RoomLike,
                label: Some("test_room".into()),
            }],
            links: vec![],
        };

        (map, geometry)
    }

    #[test]
    fn build_produces_one_plan_per_room() {
        let (map, geometry) = make_test_map_and_geometry();
        let plans = build_interior_plans(&geometry, &map, None);
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].space_id, SpaceId(0));
    }

    #[test]
    fn build_detects_doors() {
        let (map, geometry) = make_test_map_and_geometry();
        let plans = build_interior_plans(&geometry, &map, None);
        assert_eq!(plans[0].doors.len(), 2);
    }

    #[test]
    fn build_computes_reserved_paths() {
        let (map, geometry) = make_test_map_and_geometry();
        let plans = build_interior_plans(&geometry, &map, None);
        // Path from west door to east door should reserve the y=4 row interior cells.
        assert!(!plans[0].reserved_paths.is_empty());
    }

    #[test]
    fn build_classifies_zones() {
        let (map, geometry) = make_test_map_and_geometry();
        let plans = build_interior_plans(&geometry, &map, None);
        assert!(!plans[0].zones.is_empty());
        // All interior floor cells should be classified.
        let total: usize = plans[0].zones.iter().map(|z| z.cells.len()).sum();
        assert_eq!(total, 25); // 5x5 interior
    }

    #[test]
    fn interior_plan_map_lookup() {
        let (map, geometry) = make_test_map_and_geometry();
        let plans = build_interior_plans(&geometry, &map, None);
        let map_lookup = interior_plan_map(&plans);
        assert!(map_lookup.contains_key(&SpaceId(0)));
    }
}
