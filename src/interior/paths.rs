//! Reserved path computation — door detection and door-to-door BFS.
//!
//! For each room, identifies doors on its boundary and computes shortest
//! paths between all door pairs. The union of these paths forms the
//! `reserved_paths` set that must never be blocked by features or
//! non-walkable scatter.
//!
//! Also provides [`compute_corridor_passthrough`] to detect corridor cells
//! that route through a room's interior without entering via a door.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::geometry::geom::{PlacedLink, Point, Rect};
use crate::tile::map::TileMap;
use crate::tile::registry::Tile;

use super::plan::PathIntent;

/// Find all door/locked-door tiles on the boundary of a room rect.
///
/// Scans only the perimeter cells of `rect` on the tile map, returning
/// positions that contain `Door` or `LockedDoor`.
pub fn find_room_doors(tiles: &TileMap, rect: Rect) -> Vec<Point> {
    let mut doors = Vec::new();

    // Top and bottom edges
    for x in rect.x..rect.x + rect.w {
        for &y in &[rect.y, rect.y + rect.h - 1] {
            if let Some(tile) = tiles.get(x, y) {
                if tile == Tile::DOOR || tile == Tile::LOCKED_DOOR {
                    doors.push(Point { x, y });
                }
            }
        }
    }

    // Left and right edges (excluding corners already checked)
    for y in rect.y + 1..rect.y + rect.h - 1 {
        for &x in &[rect.x, rect.x + rect.w - 1] {
            if let Some(tile) = tiles.get(x, y) {
                if tile == Tile::DOOR || tile == Tile::LOCKED_DOOR {
                    doors.push(Point { x, y });
                }
            }
        }
    }

    doors
}

/// Compute reserved paths between all pairs of doors in a room.
///
/// Uses BFS to find the shortest walkable path between each door pair,
/// constrained to cells within or on the boundary of the room rect.
/// Returns individual [`PathIntent`]s and the union of all path cells.
pub fn compute_reserved_paths(
    tiles: &TileMap,
    rect: Rect,
    doors: &[Point],
) -> (Vec<PathIntent>, HashSet<Point>) {
    let mut all_reserved = HashSet::new();
    let mut intents = Vec::new();

    if doors.len() < 2 {
        return (intents, all_reserved);
    }

    // BFS from each door to every other door (only forward pairs to avoid duplication).
    for i in 0..doors.len() {
        for j in (i + 1)..doors.len() {
            if let Some(path) = bfs_path(tiles, rect, doors[i], doors[j]) {
                for &cell in &path {
                    all_reserved.insert(cell);
                }
                intents.push(PathIntent {
                    from: doors[i],
                    to: doors[j],
                    cells: path,
                });
            }
        }
    }

    (intents, all_reserved)
}

/// BFS shortest path from `start` to `end`, constrained to walkable tiles
/// within the room rect (inclusive of boundary for doors).
///
/// Returns the interior path cells (excludes `start` and `end` themselves,
/// since those are door cells).
fn bfs_path(tiles: &TileMap, rect: Rect, start: Point, end: Point) -> Option<Vec<Point>> {
    if start == end {
        return Some(vec![]);
    }

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut came_from: HashMap<Point, Point> = HashMap::new();

    visited.insert(start);
    queue.push_back(start);

    while let Some(current) = queue.pop_front() {
        if current == end {
            // Reconstruct path (excluding start and end).
            let mut path = Vec::new();
            let mut node = end;
            while node != start {
                if node != end {
                    path.push(node);
                }
                node = came_from[&node];
            }
            path.reverse();
            return Some(path);
        }

        for neighbor in current.cardinals() {
            if visited.contains(&neighbor) {
                continue;
            }

            // Must be within the room rect (inclusive boundary for doors).
            if !point_in_rect(neighbor, rect) {
                continue;
            }

            // Must be walkable.
            if let Some(tile) = tiles.get(neighbor.x, neighbor.y) {
                if tile.is_walkable() {
                    visited.insert(neighbor);
                    came_from.insert(neighbor, current);
                    queue.push_back(neighbor);
                }
            }
        }
    }

    None // No path found
}

/// Check if a point is within a rect (inclusive of boundary).
fn point_in_rect(p: Point, rect: Rect) -> bool {
    p.x >= rect.x && p.x < rect.x + rect.w && p.y >= rect.y && p.y < rect.y + rect.h
}

/// Compute corridor cells that pass through a room's interior.
///
/// A corridor "passes through" a room when its carved floor cells land inside
/// the room's rect but are NOT on the boundary (where doors live). These cells
/// must be reserved to prevent blocking features or non-walkable scatter from
/// severing corridor connectivity.
///
/// The `doors` list is used to avoid double-counting legitimate entry points.
pub fn compute_corridor_passthrough(
    rect: Rect,
    links: &[PlacedLink],
    doors: &[Point],
) -> HashSet<Point> {
    let door_set: HashSet<Point> = doors.iter().copied().collect();
    let mut passthrough = HashSet::new();

    for link in links {
        for window in link.points.windows(2) {
            let a = &window[0];
            let b = &window[1];

            // Generate all cells along this corridor segment.
            let cells: Vec<Point> = if a.x == b.x {
                let (from, to) = if a.y <= b.y { (a.y, b.y) } else { (b.y, a.y) };
                (from..=to).map(|y| Point { x: a.x, y }).collect()
            } else if a.y == b.y {
                let (from, to) = if a.x <= b.x { (a.x, b.x) } else { (b.x, a.x) };
                (from..=to).map(|x| Point { x, y: a.y }).collect()
            } else {
                continue; // Non-axis-aligned segments (shouldn't happen)
            };

            for cell in cells {
                // Cell must be inside the rect (not outside).
                if cell.x < rect.x
                    || cell.x >= rect.x + rect.w
                    || cell.y < rect.y
                    || cell.y >= rect.y + rect.h
                {
                    continue;
                }

                // Skip boundary cells (those are where doors live).
                if rect.point_on_boundary(&cell) {
                    continue;
                }

                // Skip cells that are actual doors (legitimate entry points).
                if door_set.contains(&cell) {
                    continue;
                }

                passthrough.insert(cell);
            }
        }
    }

    passthrough
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a 7x7 map with a room at (0,0,7,7):
    /// Walls on perimeter, floor inside, door on east at (6,3) and west at (0,3).
    fn make_two_door_room() -> (TileMap, Rect) {
        let mut map = TileMap::new(7, 7);
        let rect = Rect {
            x: 0,
            y: 0,
            w: 7,
            h: 7,
        };

        // Walls
        for x in 0..7 {
            map.set(x, 0, Tile::WALL);
            map.set(x, 6, Tile::WALL);
        }
        for y in 0..7 {
            map.set(0, y, Tile::WALL);
            map.set(6, y, Tile::WALL);
        }

        // Floor interior
        for y in 1..6 {
            for x in 1..6 {
                map.set(x, y, Tile::FLOOR);
            }
        }

        // Doors
        map.set(0, 3, Tile::DOOR);
        map.set(6, 3, Tile::DOOR);

        (map, rect)
    }

    #[test]
    fn find_room_doors_detects_boundary_doors() {
        let (map, rect) = make_two_door_room();
        let doors = find_room_doors(&map, rect);
        assert_eq!(doors.len(), 2);
        assert!(doors.contains(&Point { x: 0, y: 3 }));
        assert!(doors.contains(&Point { x: 6, y: 3 }));
    }

    #[test]
    fn find_room_doors_empty_when_no_doors() {
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

        let doors = find_room_doors(&map, rect);
        assert!(doors.is_empty());
    }

    #[test]
    fn compute_reserved_paths_finds_path_between_doors() {
        let (map, rect) = make_two_door_room();
        let doors = find_room_doors(&map, rect);

        let (intents, reserved) = compute_reserved_paths(&map, rect, &doors);

        assert_eq!(intents.len(), 1);
        assert!(!reserved.is_empty());
        // The path should go through the interior (y=3 row).
        for x in 1..6 {
            assert!(
                reserved.contains(&Point { x, y: 3 }),
                "expected ({}, 3) to be reserved",
                x
            );
        }
    }

    #[test]
    fn compute_reserved_paths_empty_for_single_door() {
        let (_map, rect) = make_two_door_room();
        let doors = vec![Point { x: 0, y: 3 }]; // Only one door

        // Need a map for this test too.
        let (map, _) = make_two_door_room();
        let (intents, reserved) = compute_reserved_paths(&map, rect, &doors);
        assert!(intents.is_empty());
        assert!(reserved.is_empty());
    }

    #[test]
    fn compute_reserved_paths_handles_three_doors() {
        let (mut map, rect) = make_two_door_room();
        // Add a third door on the south wall.
        map.set(3, 6, Tile::DOOR);

        let doors = find_room_doors(&map, rect);
        assert_eq!(doors.len(), 3);

        let (intents, reserved) = compute_reserved_paths(&map, rect, &doors);
        // 3 doors -> 3 pairs: (0,1), (0,2), (1,2)
        assert_eq!(intents.len(), 3);
        assert!(!reserved.is_empty());
    }

    // --- compute_corridor_passthrough tests ---

    use crate::geometry::geom::{LinkKind, PlacedLink};

    #[test]
    fn corridor_passthrough_detects_vertical_segment_through_room() {
        // Room at (0,0,7,7). Corridor passes through at x=3 from y=-5 to y=12.
        let rect = Rect {
            x: 0,
            y: 0,
            w: 7,
            h: 7,
        };
        let links = vec![PlacedLink {
            kind: LinkKind::Normal,
            points: vec![Point { x: 3, y: -5 }, Point { x: 3, y: 12 }],
        }];
        let doors = vec![]; // No doors

        let passthrough = compute_corridor_passthrough(rect, &links, &doors);

        // Interior cells at x=3 are y=1..=5 (boundary is y=0 and y=6)
        assert_eq!(passthrough.len(), 5);
        for y in 1..=5 {
            assert!(
                passthrough.contains(&Point { x: 3, y }),
                "expected (3, {}) in passthrough",
                y
            );
        }
    }

    #[test]
    fn corridor_passthrough_skips_boundary_cells() {
        let rect = Rect {
            x: 0,
            y: 0,
            w: 5,
            h: 5,
        };
        let links = vec![PlacedLink {
            kind: LinkKind::Normal,
            points: vec![Point { x: 2, y: -1 }, Point { x: 2, y: 6 }],
        }];
        let doors = vec![];

        let passthrough = compute_corridor_passthrough(rect, &links, &doors);

        // Boundary cells (2,0) and (2,4) should NOT be in passthrough.
        assert!(!passthrough.contains(&Point { x: 2, y: 0 }));
        assert!(!passthrough.contains(&Point { x: 2, y: 4 }));
        // Interior cells (2,1), (2,2), (2,3) should be.
        assert_eq!(passthrough.len(), 3);
    }

    #[test]
    fn corridor_passthrough_skips_doors() {
        let rect = Rect {
            x: 0,
            y: 0,
            w: 7,
            h: 7,
        };
        let links = vec![PlacedLink {
            kind: LinkKind::Normal,
            points: vec![Point { x: 3, y: -2 }, Point { x: 3, y: 9 }],
        }];
        // Pretend (3,3) is a door inside the room
        let doors = vec![Point { x: 3, y: 3 }];

        let passthrough = compute_corridor_passthrough(rect, &links, &doors);

        // (3,3) is a door — should not be in passthrough
        assert!(!passthrough.contains(&Point { x: 3, y: 3 }));
        // Other interior cells should be
        assert!(passthrough.contains(&Point { x: 3, y: 1 }));
        assert!(passthrough.contains(&Point { x: 3, y: 2 }));
        assert!(passthrough.contains(&Point { x: 3, y: 4 }));
        assert!(passthrough.contains(&Point { x: 3, y: 5 }));
    }

    #[test]
    fn corridor_passthrough_horizontal_segment() {
        let rect = Rect {
            x: 5,
            y: 5,
            w: 7,
            h: 7,
        };
        let links = vec![PlacedLink {
            kind: LinkKind::Normal,
            points: vec![Point { x: 2, y: 8 }, Point { x: 15, y: 8 }],
        }];
        let doors = vec![];

        let passthrough = compute_corridor_passthrough(rect, &links, &doors);

        // At y=8, interior x-range is 6..=10 (boundary at x=5 and x=11)
        assert_eq!(passthrough.len(), 5);
        for x in 6..=10 {
            assert!(passthrough.contains(&Point { x, y: 8 }));
        }
    }

    #[test]
    fn corridor_passthrough_empty_when_no_overlap() {
        let rect = Rect {
            x: 0,
            y: 0,
            w: 5,
            h: 5,
        };
        // Corridor outside the room entirely
        let links = vec![PlacedLink {
            kind: LinkKind::Normal,
            points: vec![Point { x: 10, y: 0 }, Point { x: 10, y: 10 }],
        }];
        let doors = vec![];

        let passthrough = compute_corridor_passthrough(rect, &links, &doors);
        assert!(passthrough.is_empty());
    }
}
