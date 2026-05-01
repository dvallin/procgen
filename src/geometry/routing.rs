use crate::geometry::geom::*;
use crate::intent::graph::EdgeRole;
use crate::spatial::plan::SpatialPlan;

/// Trait for routing corridors between placed spaces.
pub trait CorridorRouter {
    fn route(&self, spatial: &SpatialPlan, placed: &[PlacedSpace]) -> Vec<PlacedLink>;
}

/// Z-shape corridor router: connects spaces with straight or Z-shaped polylines.
pub struct ZShapeRouter;

impl CorridorRouter for ZShapeRouter {
    fn route(&self, spatial: &SpatialPlan, placed: &[PlacedSpace]) -> Vec<PlacedLink> {
        route_links(spatial, placed)
    }
}

fn route_links(spatial: &SpatialPlan, placed: &[PlacedSpace]) -> Vec<PlacedLink> {
    let mut links = Vec::new();

    for link in &spatial.links {
        let from = placed.iter().find(|p| p.space_id == link.from).unwrap();
        let to = placed.iter().find(|p| p.space_id == link.to).unwrap();

        let (start, end) = compute_link_endpoints(from, to);

        // Determine whether the exit is on a horizontal face (east/west)
        // or vertical face (north/south).
        let exits_horizontal = start.x == from.rect.x || start.x == from.rect.x + from.rect.w - 1;

        let points = if start.x == end.x || start.y == end.y {
            // Straight line — no bend needed.
            vec![start, end]
        } else if exits_horizontal {
            // Exit east/west: Z-shape going horizontal, then vertical, then horizontal.
            let mid_x = find_safe_transfer_x(
                start.x,
                end.x,
                start.y.min(end.y),
                start.y.max(end.y),
                placed,
            );
            vec![
                start,
                Point {
                    x: mid_x,
                    y: start.y,
                },
                Point { x: mid_x, y: end.y },
                end,
            ]
        } else {
            // Exit north/south: Z-shape going vertical, then horizontal, then vertical.
            let mid_y = find_safe_transfer_y(
                start.y,
                end.y,
                start.x.min(end.x),
                start.x.max(end.x),
                placed,
            );
            vec![
                start,
                Point {
                    x: start.x,
                    y: mid_y,
                },
                Point { x: end.x, y: mid_y },
                end,
            ]
        };

        let kind = match link.role {
            EdgeRole::Traversal => LinkKind::Normal,
            EdgeRole::RestrictedTraversal => LinkKind::Restricted,
            EdgeRole::OptionalTraversal => LinkKind::Optional,
            EdgeRole::SecretTraversal => LinkKind::Optional,
            EdgeRole::VerticalTraversal => LinkKind::Normal,
        };

        links.push(PlacedLink { kind, points });
    }

    links
}

/// Find a safe x-coordinate for a vertical transfer segment of a Z-shaped corridor.
/// The transfer segment spans from `y_min` to `y_max` at the chosen x.
/// Returns an x between `x_from` and `x_to` that doesn't intersect any room.
fn find_safe_transfer_x(
    x_from: i32,
    x_to: i32,
    y_min: i32,
    y_max: i32,
    rooms: &[PlacedSpace],
) -> i32 {
    let (lo, hi) = if x_from <= x_to {
        (x_from, x_to)
    } else {
        (x_to, x_from)
    };

    // Try midpoint first (most aesthetically centered).
    let mid = (lo + hi) / 2;
    if !intersects_any_room_x(mid, y_min, y_max, rooms) {
        return mid;
    }

    // Search outward from midpoint for a clear x.
    for offset in 1..=(hi - lo) {
        let try_left = mid - offset;
        if try_left >= lo && !intersects_any_room_x(try_left, y_min, y_max, rooms) {
            return try_left;
        }
        let try_right = mid + offset;
        if try_right <= hi && !intersects_any_room_x(try_right, y_min, y_max, rooms) {
            return try_right;
        }
    }

    // Fallback: use midpoint anyway (will conflict but at least connects).
    mid
}

/// Find a safe y-coordinate for a horizontal transfer segment of a Z-shaped corridor.
/// The transfer segment spans from `x_min` to `x_max` at the chosen y.
/// Returns a y between `y_from` and `y_to` that doesn't intersect any room.
fn find_safe_transfer_y(
    y_from: i32,
    y_to: i32,
    x_min: i32,
    x_max: i32,
    rooms: &[PlacedSpace],
) -> i32 {
    let (lo, hi) = if y_from <= y_to {
        (y_from, y_to)
    } else {
        (y_to, y_from)
    };

    // Try midpoint first.
    let mid = (lo + hi) / 2;
    if !intersects_any_room_y(mid, x_min, x_max, rooms) {
        return mid;
    }

    // Search outward from midpoint for a clear y.
    for offset in 1..=(hi - lo) {
        let try_up = mid - offset;
        if try_up >= lo && !intersects_any_room_y(try_up, x_min, x_max, rooms) {
            return try_up;
        }
        let try_down = mid + offset;
        if try_down <= hi && !intersects_any_room_y(try_down, x_min, x_max, rooms) {
            return try_down;
        }
    }

    // Fallback: use midpoint anyway.
    mid
}

/// Check if a vertical line at `x` spanning `y_min..=y_max` intersects any room.
fn intersects_any_room_x(x: i32, y_min: i32, y_max: i32, rooms: &[PlacedSpace]) -> bool {
    rooms.iter().any(|room| {
        let r = &room.rect;
        x >= r.x && x < r.x + r.w && y_max >= r.y && y_min < r.y + r.h
    })
}

/// Check if a horizontal line at `y` spanning `x_min..=x_max` intersects any room.
fn intersects_any_room_y(y: i32, x_min: i32, x_max: i32, rooms: &[PlacedSpace]) -> bool {
    rooms.iter().any(|room| {
        let r = &room.rect;
        y >= r.y && y < r.y + r.h && x_max >= r.x && x_min < r.x + r.w
    })
}

/// Determine connection points between two placed spaces.
/// Prefers straight corridors when rooms share a y-range (horizontal) or
/// x-range (vertical). Falls back to offset endpoints that require Z-routing
/// only when rooms don't overlap in the perpendicular axis.
fn compute_link_endpoints(from: &PlacedSpace, to: &PlacedSpace) -> (Point, Point) {
    let dx = to.rect.center().x - from.rect.center().x;
    let dy = to.rect.center().y - from.rect.center().y;

    if dx.abs() >= dy.abs() {
        // Primarily horizontal relationship
        if dx >= 0 {
            // from exits east, to enters west
            if let Some(shared) = from.rect.shared_y(&to.rect) {
                // Rooms overlap in y — straight horizontal corridor
                (
                    Point {
                        x: from.rect.x + from.rect.w - 1,
                        y: shared,
                    },
                    Point {
                        x: to.rect.x,
                        y: shared,
                    },
                )
            } else {
                // No y overlap — offset endpoints, will need Z-routing
                let from_y = clamp_to_interior_y(&from.rect, to.rect.center().y);
                let to_y = clamp_to_interior_y(&to.rect, from.rect.center().y);
                (
                    Point {
                        x: from.rect.x + from.rect.w - 1,
                        y: from_y,
                    },
                    Point {
                        x: to.rect.x,
                        y: to_y,
                    },
                )
            }
        } else {
            // from exits west, to enters east
            if let Some(shared) = from.rect.shared_y(&to.rect) {
                (
                    Point {
                        x: from.rect.x,
                        y: shared,
                    },
                    Point {
                        x: to.rect.x + to.rect.w - 1,
                        y: shared,
                    },
                )
            } else {
                let from_y = clamp_to_interior_y(&from.rect, to.rect.center().y);
                let to_y = clamp_to_interior_y(&to.rect, from.rect.center().y);
                (
                    Point {
                        x: from.rect.x,
                        y: from_y,
                    },
                    Point {
                        x: to.rect.x + to.rect.w - 1,
                        y: to_y,
                    },
                )
            }
        }
    } else {
        // Primarily vertical relationship
        if dy < 0 {
            // from exits north, to enters south
            if let Some(shared) = from.rect.shared_x(&to.rect) {
                (
                    Point {
                        x: shared,
                        y: from.rect.y,
                    },
                    Point {
                        x: shared,
                        y: to.rect.y + to.rect.h - 1,
                    },
                )
            } else {
                let from_x = clamp_to_interior_x(&from.rect, to.rect.center().x);
                let to_x = clamp_to_interior_x(&to.rect, from.rect.center().x);
                (
                    Point {
                        x: from_x,
                        y: from.rect.y,
                    },
                    Point {
                        x: to_x,
                        y: to.rect.y + to.rect.h - 1,
                    },
                )
            }
        } else {
            // from exits south, to enters north
            if let Some(shared) = from.rect.shared_x(&to.rect) {
                (
                    Point {
                        x: shared,
                        y: from.rect.y + from.rect.h - 1,
                    },
                    Point {
                        x: shared,
                        y: to.rect.y,
                    },
                )
            } else {
                let from_x = clamp_to_interior_x(&from.rect, to.rect.center().x);
                let to_x = clamp_to_interior_x(&to.rect, from.rect.center().x);
                (
                    Point {
                        x: from_x,
                        y: from.rect.y + from.rect.h - 1,
                    },
                    Point {
                        x: to_x,
                        y: to.rect.y,
                    },
                )
            }
        }
    }
}

/// Clamp a y-coordinate to the interior range of a rect (excluding corners).
fn clamp_to_interior_y(rect: &Rect, target_y: i32) -> i32 {
    target_y.clamp(rect.y + 1, rect.y + rect.h - 2)
}

/// Clamp an x-coordinate to the interior range of a rect (excluding corners).
fn clamp_to_interior_x(rect: &Rect, target_x: i32) -> i32 {
    target_x.clamp(rect.x + 1, rect.x + rect.w - 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::graph::{EdgeRole, NodeRole, ScenarioNodeId};
    use crate::spatial::plan::{
        AtomicSpace, RealizationStyle, SizeHint, SpaceArchetype, SpaceId, SpaceKind, SpaceLink,
        SpaceSpec, SpatialPlan,
    };

    /// Helper: build a minimal SpatialPlan with two spaces linked together.
    fn two_space_plan() -> SpatialPlan {
        SpatialPlan {
            spaces: vec![
                SpaceSpec {
                    id: SpaceId(0),
                    origin: ScenarioNodeId(0),
                    kind: SpaceKind::Atomic(AtomicSpace {
                        width: 7,
                        height: 7,
                    }),
                    role: NodeRole::Entry,
                    archetype: Some(SpaceArchetype::Chamber),
                    size_hint: SizeHint::Medium,
                    style: RealizationStyle::RoomLike,
                    tags: vec![],
                    label: None,
                },
                SpaceSpec {
                    id: SpaceId(1),
                    origin: ScenarioNodeId(1),
                    kind: SpaceKind::Atomic(AtomicSpace {
                        width: 7,
                        height: 7,
                    }),
                    role: NodeRole::Branch,
                    archetype: Some(SpaceArchetype::Chamber),
                    size_hint: SizeHint::Medium,
                    style: RealizationStyle::RoomLike,
                    tags: vec![],
                    label: None,
                },
            ],
            links: vec![SpaceLink {
                from: SpaceId(0),
                to: SpaceId(1),
                role: EdgeRole::Traversal,
                tags: vec![],
            }],
            constraints: vec![],
        }
    }

    #[test]
    fn horizontal_aligned_produces_straight_corridor() {
        let spatial = two_space_plan();

        // Two 7×7 rooms at the same y, separated horizontally.
        let r0 = Rect {
            x: 0,
            y: 0,
            w: 7,
            h: 7,
        };
        let r1 = Rect {
            x: 12,
            y: 0,
            w: 7,
            h: 7,
        };
        let placed = vec![
            PlacedSpace {
                space_id: SpaceId(0),
                rect: r0,
                footprint: Footprint::Rect(r0),
                style: RealizationStyle::RoomLike,
                label: None,
            },
            PlacedSpace {
                space_id: SpaceId(1),
                rect: r1,
                footprint: Footprint::Rect(r1),
                style: RealizationStyle::RoomLike,
                label: None,
            },
        ];

        let router = ZShapeRouter;
        let links = router.route(&spatial, &placed);

        assert_eq!(links.len(), 1);
        let corridor = &links[0];
        // Straight horizontal corridor: exactly 2 points with same y.
        assert_eq!(corridor.points.len(), 2);
        assert_eq!(corridor.points[0].y, corridor.points[1].y);
    }

    #[test]
    fn vertical_aligned_produces_straight_corridor() {
        let spatial = two_space_plan();

        // Two 7×7 rooms at the same x, separated vertically.
        let r0 = Rect {
            x: 0,
            y: 0,
            w: 7,
            h: 7,
        };
        let r1 = Rect {
            x: 0,
            y: 12,
            w: 7,
            h: 7,
        };
        let placed = vec![
            PlacedSpace {
                space_id: SpaceId(0),
                rect: r0,
                footprint: Footprint::Rect(r0),
                style: RealizationStyle::RoomLike,
                label: None,
            },
            PlacedSpace {
                space_id: SpaceId(1),
                rect: r1,
                footprint: Footprint::Rect(r1),
                style: RealizationStyle::RoomLike,
                label: None,
            },
        ];

        let router = ZShapeRouter;
        let links = router.route(&spatial, &placed);

        assert_eq!(links.len(), 1);
        let corridor = &links[0];
        // Straight vertical corridor: exactly 2 points with same x.
        assert_eq!(corridor.points.len(), 2);
        assert_eq!(corridor.points[0].x, corridor.points[1].x);
    }

    #[test]
    fn non_aligned_produces_z_shaped_corridor() {
        let spatial = two_space_plan();

        // Two 7×7 rooms that don't share an x or y interior range.
        let r0 = Rect {
            x: 0,
            y: 0,
            w: 7,
            h: 7,
        };
        let r1 = Rect {
            x: 12,
            y: 12,
            w: 7,
            h: 7,
        };
        let placed = vec![
            PlacedSpace {
                space_id: SpaceId(0),
                rect: r0,
                footprint: Footprint::Rect(r0),
                style: RealizationStyle::RoomLike,
                label: None,
            },
            PlacedSpace {
                space_id: SpaceId(1),
                rect: r1,
                footprint: Footprint::Rect(r1),
                style: RealizationStyle::RoomLike,
                label: None,
            },
        ];

        let router = ZShapeRouter;
        let links = router.route(&spatial, &placed);

        assert_eq!(links.len(), 1);
        let corridor = &links[0];
        // Z-shaped corridor: 4 points (start, bend1, bend2, end)
        assert!(
            corridor.points.len() >= 3 && corridor.points.len() <= 4,
            "Expected 3 or 4 points for Z-shape, got {}",
            corridor.points.len()
        );
    }
}
