//! Zone classification — partitions room interior cells into spatial zones.
//!
//! Each floor cell in a room is assigned to exactly one [`ZoneKind`] based on
//! its spatial relationship to walls, the room center, and reserved paths.

use std::collections::HashSet;

use crate::geometry::geom::{Point, Rect};
use crate::spatial::plan::SpaceArchetype;
use crate::tile::map::TileMap;
use crate::tile::registry::Tile;

use super::plan::{Zone, ZoneKind};

/// Classify all walkable interior cells of a room into zones.
///
/// Classification priority (first match wins):
/// 1. **DoorPath** — cell is in the `reserved_paths` set.
/// 2. **EdgeZone/Frontage** — (urban archetypes only) cells near room edges.
/// 3. **Center** — cell is within `center_radius` of the room's geometric center.
/// 4. **Corner** — cell is within 2 tiles of two perpendicular walls.
/// 5. **WallBand** — cell has at least one cardinal neighbor that is a wall.
/// 6. **Open** — everything else.
///
/// Only `Tile::FLOOR` cells inside the room (excluding boundary) are classified.
///
/// When `archetype` is `Some`, urban-specific zone kinds are applied:
/// - **Street/Alley**: cells within 1–2 tiles of the long edges become `EdgeZone`,
///   the center strip remains walkable (`DoorPath` or `Open`).
/// - **Plaza**: cells within 2 tiles of all edges become `EdgeZone`, center stays `Center`/`Open`.
/// - **Shop/Warehouse**: cells on the street-facing wall band become `Frontage`,
///   rest follow normal classification.
pub fn classify_zones(
    tiles: &TileMap,
    rect: Rect,
    reserved_paths: &HashSet<Point>,
    archetype: Option<SpaceArchetype>,
) -> Vec<Zone> {
    let center = rect.center();
    let center_radius = compute_center_radius(rect);

    let mut door_path_cells = Vec::new();
    let mut edge_zone_cells = Vec::new();
    let mut frontage_cells = Vec::new();
    let mut center_cells = Vec::new();
    let mut corner_cells = Vec::new();
    let mut wall_band_cells = Vec::new();
    let mut open_cells = Vec::new();

    // Determine urban edge classification parameters.
    let urban_mode = archetype.and_then(|a| urban_edge_mode(a, rect));

    // Iterate interior cells (skip boundary which is walls/doors).
    let x_start = rect.x + 1;
    let x_end = rect.x + rect.w - 1;
    let y_start = rect.y + 1;
    let y_end = rect.y + rect.h - 1;

    for y in y_start..y_end {
        for x in x_start..x_end {
            let p = Point { x, y };

            // Only classify walkable floor tiles.
            if tiles.get(x, y) != Some(Tile::FLOOR) {
                continue;
            }

            // Priority 1: reserved path
            if reserved_paths.contains(&p) {
                door_path_cells.push(p);
                continue;
            }

            // Priority 2: urban edge/frontage zones
            if let Some(ref mode) = urban_mode {
                match classify_urban_cell(mode, p, rect) {
                    Some(ZoneKind::EdgeZone) => {
                        edge_zone_cells.push(p);
                        continue;
                    }
                    Some(ZoneKind::Frontage) => {
                        frontage_cells.push(p);
                        continue;
                    }
                    _ => {} // fall through to normal classification
                }
            }

            // Priority 3: center
            let dx = (p.x - center.x).abs();
            let dy = (p.y - center.y).abs();
            if dx <= center_radius && dy <= center_radius {
                center_cells.push(p);
                continue;
            }

            // Priority 4: corner (near two perpendicular walls)
            if is_corner_cell(tiles, p) {
                corner_cells.push(p);
                continue;
            }

            // Priority 5: wall band (adjacent to at least one wall)
            if is_wall_adjacent(tiles, p) {
                wall_band_cells.push(p);
                continue;
            }

            // Priority 6: open
            open_cells.push(p);
        }
    }

    let mut zones = Vec::new();
    if !door_path_cells.is_empty() {
        zones.push(Zone {
            kind: ZoneKind::DoorPath,
            cells: door_path_cells,
        });
    }
    if !edge_zone_cells.is_empty() {
        zones.push(Zone {
            kind: ZoneKind::EdgeZone,
            cells: edge_zone_cells,
        });
    }
    if !frontage_cells.is_empty() {
        zones.push(Zone {
            kind: ZoneKind::Frontage,
            cells: frontage_cells,
        });
    }
    if !center_cells.is_empty() {
        zones.push(Zone {
            kind: ZoneKind::Center,
            cells: center_cells,
        });
    }
    if !corner_cells.is_empty() {
        zones.push(Zone {
            kind: ZoneKind::Corner,
            cells: corner_cells,
        });
    }
    if !wall_band_cells.is_empty() {
        zones.push(Zone {
            kind: ZoneKind::WallBand,
            cells: wall_band_cells,
        });
    }
    if !open_cells.is_empty() {
        zones.push(Zone {
            kind: ZoneKind::Open,
            cells: open_cells,
        });
    }

    zones
}

/// Describes how urban edge zones should be classified for a given archetype.
enum UrbanEdgeMode {
    /// Street/Alley: edge cells along the long edges become EdgeZone.
    /// `edge_depth` is how many tiles inward from the long edges count.
    /// `is_horizontal` indicates whether the long axis is horizontal.
    LinearEdge {
        edge_depth: i32,
        is_horizontal: bool,
    },
    /// Plaza: cells within `edge_depth` tiles of any edge become EdgeZone.
    AllEdges { edge_depth: i32 },
    /// Shop/Warehouse: cells in the wall band on the south side (street-facing) become Frontage.
    StreetFrontage,
}

/// Determine the urban edge mode for an archetype, if applicable.
fn urban_edge_mode(archetype: SpaceArchetype, rect: Rect) -> Option<UrbanEdgeMode> {
    match archetype {
        SpaceArchetype::Street => {
            let interior_w = rect.w - 2;
            let interior_h = rect.h - 2;
            let is_horizontal = interior_w >= interior_h;
            // Streets get 2 tiles of edge zone along long edges.
            Some(UrbanEdgeMode::LinearEdge {
                edge_depth: 2,
                is_horizontal,
            })
        }
        SpaceArchetype::Alley => {
            let interior_w = rect.w - 2;
            let interior_h = rect.h - 2;
            let is_horizontal = interior_w >= interior_h;
            // Alleys are narrow — only 1 tile of edge zone.
            Some(UrbanEdgeMode::LinearEdge {
                edge_depth: 1,
                is_horizontal,
            })
        }
        SpaceArchetype::Plaza => Some(UrbanEdgeMode::AllEdges { edge_depth: 2 }),
        SpaceArchetype::Shop | SpaceArchetype::Warehouse => Some(UrbanEdgeMode::StreetFrontage),
        _ => None,
    }
}

/// Classify a single cell according to urban edge rules.
/// Returns `Some(ZoneKind)` if the cell falls in an urban-specific zone, `None` otherwise.
fn classify_urban_cell(mode: &UrbanEdgeMode, p: Point, rect: Rect) -> Option<ZoneKind> {
    let x_start = rect.x + 1;
    let x_end = rect.x + rect.w - 2; // last interior x
    let y_start = rect.y + 1;
    let y_end = rect.y + rect.h - 2; // last interior y

    match mode {
        UrbanEdgeMode::LinearEdge {
            edge_depth,
            is_horizontal,
        } => {
            if *is_horizontal {
                // Long axis is X — edges are top and bottom (Y boundaries)
                let dist_from_top = p.y - y_start;
                let dist_from_bottom = y_end - p.y;
                if dist_from_top < *edge_depth || dist_from_bottom < *edge_depth {
                    return Some(ZoneKind::EdgeZone);
                }
            } else {
                // Long axis is Y — edges are left and right (X boundaries)
                let dist_from_left = p.x - x_start;
                let dist_from_right = x_end - p.x;
                if dist_from_left < *edge_depth || dist_from_right < *edge_depth {
                    return Some(ZoneKind::EdgeZone);
                }
            }
            None
        }
        UrbanEdgeMode::AllEdges { edge_depth } => {
            let dist_from_top = p.y - y_start;
            let dist_from_bottom = y_end - p.y;
            let dist_from_left = p.x - x_start;
            let dist_from_right = x_end - p.x;
            let min_dist = dist_from_top
                .min(dist_from_bottom)
                .min(dist_from_left)
                .min(dist_from_right);
            if min_dist < *edge_depth {
                return Some(ZoneKind::EdgeZone);
            }
            None
        }
        UrbanEdgeMode::StreetFrontage => {
            // Street-facing = south wall band (cells adjacent to the south wall).
            // We use the last interior row as frontage.
            if p.y == y_end {
                return Some(ZoneKind::Frontage);
            }
            None
        }
    }
}

/// Compute the center radius based on room dimensions.
///
/// For small rooms (interior <= 3), radius is 0 (just the center point).
/// For medium rooms, radius is 1.
/// For large rooms, radius is 2.
fn compute_center_radius(rect: Rect) -> i32 {
    let interior_w = rect.w - 2; // Subtract walls
    let interior_h = rect.h - 2;
    let min_dim = interior_w.min(interior_h);

    if min_dim <= 3 {
        0
    } else if min_dim <= 7 {
        1
    } else {
        2
    }
}

/// Is this cell near two perpendicular walls (within 2 tiles of each)?
fn is_corner_cell(tiles: &TileMap, p: Point) -> bool {
    let near_north = (1..=2).any(|dy| tiles.get(p.x, p.y - dy) == Some(Tile::WALL));
    let near_south = (1..=2).any(|dy| tiles.get(p.x, p.y + dy) == Some(Tile::WALL));
    let near_west = (1..=2).any(|dx| tiles.get(p.x - dx, p.y) == Some(Tile::WALL));
    let near_east = (1..=2).any(|dx| tiles.get(p.x + dx, p.y) == Some(Tile::WALL));

    let near_vertical = near_north || near_south;
    let near_horizontal = near_west || near_east;

    near_vertical && near_horizontal
}

/// Does this cell have at least one cardinal neighbor that is a wall?
fn is_wall_adjacent(tiles: &TileMap, p: Point) -> bool {
    p.cardinals()
        .iter()
        .any(|n| tiles.get(n.x, n.y) == Some(Tile::WALL))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 7x7 room: walls on perimeter, floor inside.
    fn make_room_map() -> (TileMap, Rect) {
        let mut map = TileMap::new(7, 7);
        let rect = Rect {
            x: 0,
            y: 0,
            w: 7,
            h: 7,
        };

        for x in 0..7 {
            map.set(x, 0, Tile::WALL);
            map.set(x, 6, Tile::WALL);
        }
        for y in 0..7 {
            map.set(0, y, Tile::WALL);
            map.set(6, y, Tile::WALL);
        }
        for y in 1..6 {
            for x in 1..6 {
                map.set(x, y, Tile::FLOOR);
            }
        }

        (map, rect)
    }

    #[test]
    fn classify_zones_all_cells_assigned() {
        let (map, rect) = make_room_map();
        let reserved = HashSet::new();

        let zones = classify_zones(&map, rect, &reserved, None);

        let total_cells: usize = zones.iter().map(|z| z.cells.len()).sum();
        // Interior is 5x5 = 25 floor cells.
        assert_eq!(total_cells, 25);
    }

    #[test]
    fn classify_zones_center_contains_middle() {
        let (map, rect) = make_room_map();
        let reserved = HashSet::new();

        let zones = classify_zones(&map, rect, &reserved, None);

        let center_zone = zones.iter().find(|z| z.kind == ZoneKind::Center);
        assert!(center_zone.is_some());
        assert!(center_zone.unwrap().cells.contains(&Point { x: 3, y: 3 }));
    }

    #[test]
    fn classify_zones_reserved_paths_become_door_path() {
        let (map, rect) = make_room_map();
        let mut reserved = HashSet::new();
        reserved.insert(Point { x: 1, y: 3 });
        reserved.insert(Point { x: 2, y: 3 });
        reserved.insert(Point { x: 3, y: 3 });

        let zones = classify_zones(&map, rect, &reserved, None);

        let door_path = zones.iter().find(|z| z.kind == ZoneKind::DoorPath);
        assert!(door_path.is_some());
        let dp = door_path.unwrap();
        assert_eq!(dp.cells.len(), 3);
        assert!(dp.cells.contains(&Point { x: 1, y: 3 }));
        assert!(dp.cells.contains(&Point { x: 2, y: 3 }));
        assert!(dp.cells.contains(&Point { x: 3, y: 3 }));
    }

    #[test]
    fn classify_zones_no_duplicates() {
        let (map, rect) = make_room_map();
        let mut reserved = HashSet::new();
        reserved.insert(Point { x: 3, y: 3 }); // Center is also reserved

        let zones = classify_zones(&map, rect, &reserved, None);

        // (3,3) should be in DoorPath, not Center.
        let door_path = zones.iter().find(|z| z.kind == ZoneKind::DoorPath).unwrap();
        assert!(door_path.cells.contains(&Point { x: 3, y: 3 }));

        if let Some(center) = zones.iter().find(|z| z.kind == ZoneKind::Center) {
            assert!(!center.cells.contains(&Point { x: 3, y: 3 }));
        }
    }

    #[test]
    fn small_room_has_zero_center_radius() {
        // 5x5 room -> interior 3x3
        let mut map = TileMap::new(5, 5);
        let rect = Rect {
            x: 0,
            y: 0,
            w: 5,
            h: 5,
        };
        for x in 0..5 {
            map.set(x, 0, Tile::WALL);
            map.set(x, 4, Tile::WALL);
        }
        for y in 0..5 {
            map.set(0, y, Tile::WALL);
            map.set(4, y, Tile::WALL);
        }
        for y in 1..4 {
            for x in 1..4 {
                map.set(x, y, Tile::FLOOR);
            }
        }

        let zones = classify_zones(&map, rect, &HashSet::new(), None);

        let center = zones.iter().find(|z| z.kind == ZoneKind::Center).unwrap();
        // Only the exact center should be classified as Center.
        assert_eq!(center.cells.len(), 1);
        assert!(center.cells.contains(&Point { x: 2, y: 2 }));
    }
}
