//! Placement strategies for feature placement within rooms.
//!
//! Each strategy produces candidate [`Point`]s inside a room's [`Rect`],
//! filtering by tile type and already-occupied cells.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::geometry::geom::{Point, Rect};
use crate::tile::map::{Tile, TileMap};

/// How a feature should be positioned inside a room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlacementStrategy {
    /// Place at (or nearest to) the geometric center of the room.
    Center,
    /// Place adjacent to a wall (at least one cardinal neighbor is `Wall`).
    WallAdjacent,
    /// Place in an interior corner (within 2 tiles of two perpendicular walls).
    Corner,
    /// Place on any available floor tile.
    RandomFloor,
}

/// Returns true if any cardinal neighbor of `p` is a Door or LockedDoor.
/// Features placed next to doors would block passage.
fn is_door_adjacent(map: &TileMap, p: Point) -> bool {
    p.cardinals()
        .iter()
        .any(|n| matches!(map.get(n.x, n.y), Some(Tile::Door) | Some(Tile::LockedDoor)))
}

/// Find the walkable `Floor` tile closest to the room center,
/// excluding doors, door-adjacent tiles, and occupied cells.
///
/// Iterates every cell inside `rect`, keeps only `Tile::Floor` tiles that
/// are not in `occupied`, and returns the one with the smallest squared
/// Euclidean distance to the rect's geometric center.
pub fn find_center_tile(map: &TileMap, rect: Rect, occupied: &HashSet<Point>) -> Option<Point> {
    let center = rect.center();

    rect.iter_points()
        .filter(|p| !occupied.contains(p))
        .filter(|p| map.get(p.x, p.y) == Some(Tile::Floor))
        .filter(|p| !is_door_adjacent(map, *p))
        .min_by_key(|p| {
            let dx = p.x - center.x;
            let dy = p.y - center.y;
            dx * dx + dy * dy
        })
}

/// Find all walkable `Floor` tiles (NOT doors) that have at least one
/// cardinal neighbor that is a `Wall` tile. Excludes occupied and
/// door-adjacent cells.
pub fn find_wall_adjacent_tiles(
    map: &TileMap,
    rect: Rect,
    occupied: &HashSet<Point>,
) -> Vec<Point> {
    rect.iter_points()
        .filter(|p| !occupied.contains(p))
        .filter(|p| map.get(p.x, p.y) == Some(Tile::Floor))
        .filter(|p| !is_door_adjacent(map, *p))
        .filter(|p| {
            p.cardinals()
                .iter()
                .any(|n| map.get(n.x, n.y) == Some(Tile::Wall))
        })
        .collect()
}

/// Find all walkable `Floor` tiles (NOT doors) in the interior corner
/// areas of the room — tiles that are within 2 cells of two perpendicular
/// walls. Excludes occupied and door-adjacent cells.
///
/// "Within 2 tiles of a wall" means the cell's distance to the nearest
/// wall on that axis is ≤ 2 (measured by how many floor cells separate
/// it from a wall in that direction).
pub fn find_corner_tiles(map: &TileMap, rect: Rect, occupied: &HashSet<Point>) -> Vec<Point> {
    rect.iter_points()
        .filter(|p| !occupied.contains(p))
        .filter(|p| map.get(p.x, p.y) == Some(Tile::Floor))
        .filter(|p| !is_door_adjacent(map, *p))
        .filter(|p| {
            let near_north_wall =
                (1..=2).any(|dy| map.get(p.x, p.y - dy).map_or(false, |t| t == Tile::Wall));
            let near_south_wall =
                (1..=2).any(|dy| map.get(p.x, p.y + dy).map_or(false, |t| t == Tile::Wall));
            let near_west_wall =
                (1..=2).any(|dx| map.get(p.x - dx, p.y).map_or(false, |t| t == Tile::Wall));
            let near_east_wall =
                (1..=2).any(|dx| map.get(p.x + dx, p.y).map_or(false, |t| t == Tile::Wall));

            let near_vertical_wall = near_north_wall || near_south_wall;
            let near_horizontal_wall = near_west_wall || near_east_wall;

            near_vertical_wall && near_horizontal_wall
        })
        .collect()
}

/// Find all walkable `Floor` tiles (NOT doors) in the room rect that
/// are not occupied and not adjacent to doors. Returns every candidate.
pub fn find_floor_tiles(map: &TileMap, rect: Rect, occupied: &HashSet<Point>) -> Vec<Point> {
    rect.iter_points()
        .filter(|p| !occupied.contains(p))
        .filter(|p| map.get(p.x, p.y) == Some(Tile::Floor))
        .filter(|p| !is_door_adjacent(map, *p))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a small room TileMap for testing.
    ///
    /// Layout (7×7, room rect is (1,1,5,5)):
    /// ```text
    ///  0123456
    /// 0.......       (Void)
    /// 1.WWWWW.
    /// 2.W...W.
    /// 3.W...D.       Door on the east wall at (5,3)
    /// 4.W...W.
    /// 5.WWWWW.
    /// 6.......
    /// ```
    fn make_test_map() -> (TileMap, Rect) {
        let mut map = TileMap::new(7, 7);
        let rect = Rect {
            x: 1,
            y: 1,
            w: 5,
            h: 5,
        };

        // Walls around perimeter of the rect.
        for x in 1..6 {
            map.set(x, 1, Tile::Wall);
            map.set(x, 5, Tile::Wall);
        }
        for y in 1..6 {
            map.set(1, y, Tile::Wall);
            map.set(5, y, Tile::Wall);
        }

        // Floor interior.
        for y in 2..5 {
            for x in 2..5 {
                map.set(x, y, Tile::Floor);
            }
        }

        // Door on east wall.
        map.set(5, 3, Tile::Door);

        (map, rect)
    }

    #[test]
    fn center_tile_finds_middle_floor() {
        let (map, rect) = make_test_map();
        let occupied = HashSet::new();

        let result = find_center_tile(&map, rect, &occupied);
        assert_eq!(result, Some(Point { x: 3, y: 3 }));
    }

    #[test]
    fn center_tile_skips_occupied() {
        let (map, rect) = make_test_map();
        let mut occupied = HashSet::new();
        occupied.insert(Point { x: 3, y: 3 });

        let result = find_center_tile(&map, rect, &occupied);
        // Should pick one of the four cardinal neighbors of center.
        let p = result.expect("should find a tile");
        assert_ne!(p, Point { x: 3, y: 3 });
        // Must still be Floor.
        assert_eq!(map.get(p.x, p.y), Some(Tile::Floor));
    }

    #[test]
    fn wall_adjacent_finds_tiles_next_to_walls() {
        let (map, rect) = make_test_map();
        let occupied = HashSet::new();

        let candidates = find_wall_adjacent_tiles(&map, rect, &occupied);
        // Floor tiles adjacent to walls: (2,2),(3,2),(4,2),(2,3),(2,4),(3,4),(4,4).
        // Note: (4,3) has a Door (not Wall) to the east, so it does NOT qualify.
        assert_eq!(candidates.len(), 7);
        for p in &candidates {
            assert_eq!(map.get(p.x, p.y), Some(Tile::Floor));
            let has_wall = p
                .cardinals()
                .iter()
                .any(|n| map.get(n.x, n.y) == Some(Tile::Wall));
            assert!(has_wall, "tile {:?} should be next to a wall", p);
        }
    }

    #[test]
    fn wall_adjacent_excludes_doors() {
        let (map, rect) = make_test_map();
        let occupied = HashSet::new();

        let candidates = find_wall_adjacent_tiles(&map, rect, &occupied);
        // The door at (5,3) must NOT appear.
        assert!(!candidates.contains(&Point { x: 5, y: 3 }));
    }

    #[test]
    fn corner_tiles_finds_perpendicular_wall_neighbors() {
        let (map, rect) = make_test_map();
        let occupied = HashSet::new();

        let candidates = find_corner_tiles(&map, rect, &occupied);
        // In our 3×3 floor interior, the four literal corners (2,2),(4,2),(2,4),(4,4)
        // are each within 1 tile of two perpendicular walls. The edge midpoints
        // (3,2),(3,4) are near a horizontal wall (N or S) and also within 2 tiles
        // of a vertical wall (W or E), so they qualify too. Similarly (2,3),(4,3).
        // All 8 floor tiles in this small room qualify.
        // The center (3,3) is also within 2 tiles of walls in both directions.
        assert!(!candidates.is_empty());
        for p in &candidates {
            assert_eq!(map.get(p.x, p.y), Some(Tile::Floor));
        }
    }

    #[test]
    fn floor_tiles_returns_all_floor_not_doors() {
        let (map, rect) = make_test_map();
        let occupied = HashSet::new();

        let candidates = find_floor_tiles(&map, rect, &occupied);
        // 3×3 interior = 9 floor tiles. Door at (5,3) is NOT Floor.
        // (4,3) is adjacent to the door, so it's excluded. 9 - 1 = 8.
        assert_eq!(candidates.len(), 8);
        for p in &candidates {
            assert_eq!(map.get(p.x, p.y), Some(Tile::Floor));
        }
        // Verify door-adjacent tile is excluded.
        assert!(!candidates.contains(&Point { x: 4, y: 3 }));
    }

    #[test]
    fn floor_tiles_excludes_occupied() {
        let (map, rect) = make_test_map();
        let mut occupied = HashSet::new();
        occupied.insert(Point { x: 2, y: 2 });
        occupied.insert(Point { x: 4, y: 4 });

        let candidates = find_floor_tiles(&map, rect, &occupied);
        // 8 eligible (see above) - 2 occupied = 6.
        assert_eq!(candidates.len(), 6);
        assert!(!candidates.contains(&Point { x: 2, y: 2 }));
        assert!(!candidates.contains(&Point { x: 4, y: 4 }));
    }
}
