//! Zone classification — partitions room interior cells into spatial zones.
//!
//! Each floor cell in a room is assigned to exactly one [`ZoneKind`] based on
//! its spatial relationship to walls, the room center, and reserved paths.

use std::collections::HashSet;

use crate::geometry::geom::{Point, Rect};
use crate::tile::map::TileMap;
use crate::tile::registry::Tile;

use super::plan::{Zone, ZoneKind};

/// Classify all walkable interior cells of a room into zones.
///
/// Classification priority (first match wins):
/// 1. **DoorPath** — cell is in the `reserved_paths` set.
/// 2. **Center** — cell is within `center_radius` of the room's geometric center.
/// 3. **Corner** — cell is within 2 tiles of two perpendicular walls.
/// 4. **WallBand** — cell has at least one cardinal neighbor that is a wall.
/// 5. **Open** — everything else.
///
/// Only `Tile::FLOOR` cells inside the room (excluding boundary) are classified.
pub fn classify_zones(tiles: &TileMap, rect: Rect, reserved_paths: &HashSet<Point>) -> Vec<Zone> {
    let center = rect.center();
    let center_radius = compute_center_radius(rect);

    let mut door_path_cells = Vec::new();
    let mut center_cells = Vec::new();
    let mut corner_cells = Vec::new();
    let mut wall_band_cells = Vec::new();
    let mut open_cells = Vec::new();

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

            // Priority 2: center
            let dx = (p.x - center.x).abs();
            let dy = (p.y - center.y).abs();
            if dx <= center_radius && dy <= center_radius {
                center_cells.push(p);
                continue;
            }

            // Priority 3: corner (near two perpendicular walls)
            if is_corner_cell(tiles, p) {
                corner_cells.push(p);
                continue;
            }

            // Priority 4: wall band (adjacent to at least one wall)
            if is_wall_adjacent(tiles, p) {
                wall_band_cells.push(p);
                continue;
            }

            // Priority 5: open
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

        let zones = classify_zones(&map, rect, &reserved);

        let total_cells: usize = zones.iter().map(|z| z.cells.len()).sum();
        // Interior is 5x5 = 25 floor cells.
        assert_eq!(total_cells, 25);
    }

    #[test]
    fn classify_zones_center_contains_middle() {
        let (map, rect) = make_room_map();
        let reserved = HashSet::new();

        let zones = classify_zones(&map, rect, &reserved);

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

        let zones = classify_zones(&map, rect, &reserved);

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

        let zones = classify_zones(&map, rect, &reserved);

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

        let zones = classify_zones(&map, rect, &HashSet::new());

        let center = zones.iter().find(|z| z.kind == ZoneKind::Center).unwrap();
        // Only the exact center should be classified as Center.
        assert_eq!(center.cells.len(), 1);
        assert!(center.cells.contains(&Point { x: 2, y: 2 }));
    }
}
