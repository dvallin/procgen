use std::collections::HashMap;

use tracing::{debug, info, info_span};

use crate::geometry::geom::*;
use crate::intent::graph::EdgeRole;
use crate::spatial::plan::SpatialPlan;

/// Trait for routing corridors between placed spaces.
pub trait CorridorRouter {
    fn route(&self, spatial: &SpatialPlan, placed: &[PlacedSpace]) -> Vec<PlacedLink>;
}

/// Z-shape corridor router: connects spaces with straight or Z-shaped polylines.
///
/// When multiple non-restricted links depart from the same room face, they share
/// a single exit point (one door) and their corridors naturally merge into a
/// trunk that branches towards each destination. Restricted and secret traversals
/// always get their own dedicated exit point and door.
pub struct ZShapeRouter;

impl CorridorRouter for ZShapeRouter {
    fn route(&self, spatial: &SpatialPlan, placed: &[PlacedSpace]) -> Vec<PlacedLink> {
        route_links(spatial, placed)
    }
}

// ── Face utilities ───────────────────────────────────────────────────────

/// Which face of a room a corridor exits or enters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Face {
    North,
    South,
    East,
    West,
}

/// Determine the exit face from `from` toward `to`, considering room collisions.
///
/// Uses the dominant axis (dx vs dy) as the natural direction. When the natural
/// path crosses 2+ intermediate rooms (a sign the direction is fundamentally
/// wrong — e.g. going north through a stacked sibling column), switches to the
/// secondary (perpendicular) axis to route around them.
fn determine_exit_face(from: &PlacedSpace, to: &PlacedSpace, placed: &[PlacedSpace]) -> Face {
    let dx = to.rect.center().x - from.rect.center().x;
    let dy = to.rect.center().y - from.rect.center().y;

    let natural = if dx.abs() >= dy.abs() {
        if dx >= 0 { Face::East } else { Face::West }
    } else if dy >= 0 {
        Face::South
    } else {
        Face::North
    };

    // If 2+ rooms block the natural-axis path, switch to the secondary axis.
    if natural_path_blocked(from, to, natural, placed) {
        return match natural {
            Face::North | Face::South => {
                if dx >= 0 {
                    Face::East
                } else {
                    Face::West
                }
            }
            Face::East | Face::West => {
                if dy >= 0 {
                    Face::South
                } else {
                    Face::North
                }
            }
        };
    }

    natural
}

/// Check if 2+ rooms block a straight path along the given exit face axis.
///
/// For vertical exits (N/S): checks a vertical line at `from.center.x`
/// spanning the y-range between the two room centers.
/// For horizontal exits (E/W): checks a horizontal line at `from.center.y`.
///
/// A single blocking room is tolerable (Z-shape routing bends around it).
/// Two or more indicate the exit direction is fundamentally wrong.
fn natural_path_blocked(
    from: &PlacedSpace,
    to: &PlacedSpace,
    exit_face: Face,
    placed: &[PlacedSpace],
) -> bool {
    let fc = from.rect.center();
    let tc = to.rect.center();
    let count = placed
        .iter()
        .filter(|room| {
            if room.space_id == from.space_id || room.space_id == to.space_id {
                return false;
            }
            let r = &room.rect;
            match exit_face {
                Face::North | Face::South => {
                    let y_lo = fc.y.min(tc.y);
                    let y_hi = fc.y.max(tc.y);
                    fc.x >= r.x && fc.x < r.x + r.w && y_hi > r.y && y_lo < r.y + r.h
                }
                Face::East | Face::West => {
                    let x_lo = fc.x.min(tc.x);
                    let x_hi = fc.x.max(tc.x);
                    fc.y >= r.y && fc.y < r.y + r.h && x_hi > r.x && x_lo < r.x + r.w
                }
            }
        })
        .count();
    count >= 2
}

/// Interior range of a room face (excluding corner cells).
///
/// Returns `(fixed_coord, range_start, range_end)`:
/// - **East/West**: fixed is x, range spans y.
/// - **North/South**: fixed is y, range spans x.
fn face_interior_range(rect: &Rect, face: Face) -> (i32, i32, i32) {
    match face {
        Face::East => (rect.x + rect.w - 1, rect.y + 1, rect.y + rect.h - 2),
        Face::West => (rect.x, rect.y + 1, rect.y + rect.h - 2),
        Face::North => (rect.y, rect.x + 1, rect.x + rect.w - 2),
        Face::South => (rect.y + rect.h - 1, rect.x + 1, rect.x + rect.w - 2),
    }
}

/// Construct a `Point` on a face from its fixed and variable coordinates.
fn point_on_face(face: Face, fixed: i32, variable: i32) -> Point {
    match face {
        Face::East | Face::West => Point {
            x: fixed,
            y: variable,
        },
        Face::North | Face::South => Point {
            x: variable,
            y: fixed,
        },
    }
}

/// The face on the opposite side of a room.
fn opposite_face(face: Face) -> Face {
    match face {
        Face::North => Face::South,
        Face::South => Face::North,
        Face::East => Face::West,
        Face::West => Face::East,
    }
}

/// Choose the entry face for a link given its exit face and exit point.
///
/// Prefers an L-shape (perpendicular entry) over a Z-shape (opposite entry)
/// whenever a straight corridor isn't possible. An L-shape uses one bend
/// instead of two, producing cleaner corridors.
fn determine_entry_face(exit_face: Face, exit_point: &Point, to_rect: &Rect) -> Face {
    let opposite = opposite_face(exit_face);
    let (_, rs, re) = face_interior_range(to_rect, opposite);

    // If exit's variable coordinate falls within the opposite face interior,
    // a straight corridor is possible → keep the natural opposite entry.
    let aligned = match exit_face {
        Face::East | Face::West => exit_point.y >= rs && exit_point.y <= re,
        Face::North | Face::South => exit_point.x >= rs && exit_point.x <= re,
    };

    if aligned {
        return opposite;
    }

    // Not aligned → perpendicular entry creates an L-shape (one bend).
    let to_center = to_rect.center();
    match exit_face {
        Face::East | Face::West => {
            if exit_point.y < to_center.y {
                Face::North
            } else {
                Face::South
            }
        }
        Face::North | Face::South => {
            if exit_point.x < to_center.x {
                Face::West
            } else {
                Face::East
            }
        }
    }
}

/// Whether a link role can be merged with other links sharing the same exit door.
/// Restricted and secret traversals need their own dedicated door.
fn is_mergeable_role(role: &EdgeRole) -> bool {
    matches!(
        role,
        EdgeRole::Traversal | EdgeRole::OptionalTraversal | EdgeRole::VerticalTraversal
    )
}

// ── Main routing ─────────────────────────────────────────────────────────

fn route_links(spatial: &SpatialPlan, placed: &[PlacedSpace]) -> Vec<PlacedLink> {
    let n = spatial.links.len();
    if n == 0 {
        return Vec::new();
    }

    let _span = info_span!("corridor_routing", links = n).entered();

    // Phase 1: determine exit face for each link.
    // (from_placed_idx, to_placed_idx, exit_face)
    let link_data: Vec<(usize, usize, Face)> = spatial
        .links
        .iter()
        .map(|link| {
            let fi = placed.iter().position(|p| p.space_id == link.from).unwrap();
            let ti = placed.iter().position(|p| p.space_id == link.to).unwrap();
            let ff = determine_exit_face(&placed[fi], &placed[ti], placed);
            (fi, ti, ff)
        })
        .collect();

    // Phase 2: group by (from_room, from_face), classify as mergeable vs isolated.
    // Value: (mergeable_link_indices, isolated_link_indices)
    let mut exit_groups: HashMap<(usize, Face), (Vec<usize>, Vec<usize>)> = HashMap::new();
    for (li, link) in spatial.links.iter().enumerate() {
        let (fi, _, ff) = link_data[li];
        let (mergeable, isolated) = exit_groups.entry((fi, ff)).or_default();
        if is_mergeable_role(&link.role) {
            mergeable.push(li);
        } else {
            isolated.push(li);
        }
    }

    for (&(from_idx, ref face), (mergeable, _isolated)) in &exit_groups {
        if mergeable.len() > 1 {
            debug!(
                space = placed[from_idx].space_id.0,
                face = ?face,
                merged_count = mergeable.len(),
                "merged exit group"
            );
        }
    }

    // Phase 3: assign exit points.
    let mut exit_points = vec![Point { x: 0, y: 0 }; n];
    for (&(from_idx, face), (mergeable, isolated)) in &exit_groups {
        let rect = &placed[from_idx].rect;
        let (fixed, rs, re) = face_interior_range(rect, face);
        let center = (rs + re) / 2;

        // Mergeable links all share the center exit point (one door).
        for &li in mergeable.iter() {
            exit_points[li] = point_on_face(face, fixed, center);
        }

        // Isolated links each get their own exit point, offset from center.
        for (i, &li) in isolated.iter().enumerate() {
            let pos = if mergeable.is_empty() && isolated.len() == 1 {
                // Only link on this face — use center.
                center
            } else {
                // Alternate sides: center−3, center+3, center−6, …
                let half = (i as i32 / 2) + 1;
                let sign = if i % 2 == 0 { -1 } else { 1 };
                (center + sign * half * 3).clamp(rs, re)
            };
            exit_points[li] = point_on_face(face, fixed, pos);
        }
    }

    // Phase 4: determine entry faces and derive entry points.
    // Uses L-shape preference: when a straight corridor isn't possible,
    // picks a perpendicular entry face (1 bend) over the opposite face (2 bends).
    let mut entry_faces = vec![Face::North; n];
    let mut entry_points = vec![Point { x: 0, y: 0 }; n];
    for (li, &(_, ti, ff)) in link_data.iter().enumerate() {
        let to_rect = &placed[ti].rect;
        let ef = determine_entry_face(ff, &exit_points[li], to_rect);
        entry_faces[li] = ef;

        let (fixed, rs, re) = face_interior_range(to_rect, ef);
        let exit_var = match ef {
            Face::East | Face::West => exit_points[li].y,
            Face::North | Face::South => exit_points[li].x,
        };
        entry_points[li] = point_on_face(ef, fixed, exit_var.clamp(rs, re));
    }

    // Phase 5: route each link.
    let mut links = Vec::new();
    for (li, link) in spatial.links.iter().enumerate() {
        let start = exit_points[li];
        let end = entry_points[li];
        let (fi, ti, ff) = link_data[li];
        let ef = entry_faces[li];

        let exit_vertical = matches!(ff, Face::North | Face::South);
        let entry_vertical = matches!(ef, Face::North | Face::South);

        let points = if start.x == end.x || start.y == end.y {
            // Straight corridor — already aligned.
            // Check if it passes through any room (excluding endpoints).
            let blocked = segment_intersects_room(start, end, placed, fi, ti);
            if blocked {
                // Deviate perpendicular to the straight line to route around.
                // A vertical straight needs H→V→H; a horizontal one needs V→H→V.
                route_z_shape(start, end, !exit_vertical, placed)
            } else {
                vec![start, end]
            }
        } else if exit_vertical != entry_vertical {
            // Perpendicular faces → try L-shape (one bend).
            // Try both possible L-shape orientations, prefer the one without
            // room intersections. Fall back to Z-shape if both are blocked.
            let bend_a = if exit_vertical {
                Point {
                    x: start.x,
                    y: end.y,
                }
            } else {
                Point {
                    x: end.x,
                    y: start.y,
                }
            };

            let seg_a1_blocked = segment_intersects_room(start, bend_a, placed, fi, ti);
            let seg_a2_blocked = segment_intersects_room(bend_a, end, placed, fi, ti);

            if !seg_a1_blocked && !seg_a2_blocked {
                // Primary L-shape is clear.
                vec![start, bend_a, end]
            } else {
                // Try alternate L-shape (bend the other way).
                let bend_b = if exit_vertical {
                    Point {
                        x: end.x,
                        y: start.y,
                    }
                } else {
                    Point {
                        x: start.x,
                        y: end.y,
                    }
                };

                let seg_b1_blocked = segment_intersects_room(start, bend_b, placed, fi, ti);
                let seg_b2_blocked = segment_intersects_room(bend_b, end, placed, fi, ti);

                if !seg_b1_blocked && !seg_b2_blocked {
                    // Alternate L-shape is clear.
                    vec![start, bend_b, end]
                } else {
                    // Both L-shapes blocked → fall back to Z-shape.
                    route_z_shape(start, end, exit_vertical, placed)
                }
            }
        } else if matches!(ff, Face::East | Face::West) {
            // Same axis, horizontal → Z-shape: H→V→H.
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
            // Same axis, vertical → Z-shape: V→H→V.
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

        let shape = match points.len() {
            2 => "straight",
            3 => "L-shape",
            4 => "Z-shape",
            _ => "complex",
        };
        debug!(
            from = link.from.0,
            to = link.to.0,
            shape = shape,
            waypoints = points.len(),
            "routed corridor"
        );

        let kind = match link.role {
            EdgeRole::Traversal => LinkKind::Normal,
            EdgeRole::RestrictedTraversal => LinkKind::Restricted,
            EdgeRole::OptionalTraversal => LinkKind::Optional,
            EdgeRole::SecretTraversal => LinkKind::Optional,
            EdgeRole::VerticalTraversal => LinkKind::Normal,
        };

        links.push(PlacedLink { kind, points });
    }

    info!(corridors = links.len(), "corridor routing complete");

    links
}

// ── Transfer-point finding ───────────────────────────────────────────────

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

// ── Collision helpers ────────────────────────────────────────────────────

/// Does a vertical line at `x` spanning `y_min..=y_max` intersect any room?
fn intersects_any_room_x(x: i32, y_min: i32, y_max: i32, rooms: &[PlacedSpace]) -> bool {
    rooms.iter().any(|room| {
        let r = &room.rect;
        x >= r.x && x < r.x + r.w && y_max >= r.y && y_min < r.y + r.h
    })
}

/// Does a horizontal line at `y` spanning `x_min..=x_max` intersect any room?
fn intersects_any_room_y(y: i32, x_min: i32, x_max: i32, rooms: &[PlacedSpace]) -> bool {
    rooms.iter().any(|room| {
        let r = &room.rect;
        y >= r.y && y < r.y + r.h && x_max >= r.x && x_min < r.x + r.w
    })
}

/// Check if an axis-aligned segment between two points passes through any room,
/// excluding the rooms at `skip_a` and `skip_b` indices (the corridor's endpoints).
fn segment_intersects_room(
    a: Point,
    b: Point,
    rooms: &[PlacedSpace],
    skip_a: usize,
    skip_b: usize,
) -> bool {
    rooms.iter().enumerate().any(|(idx, room)| {
        if idx == skip_a || idx == skip_b {
            return false;
        }
        let r = &room.rect;
        if a.x == b.x {
            // Vertical segment
            let y_min = a.y.min(b.y);
            let y_max = a.y.max(b.y);
            a.x >= r.x && a.x < r.x + r.w && y_max >= r.y && y_min < r.y + r.h
        } else if a.y == b.y {
            // Horizontal segment
            let x_min = a.x.min(b.x);
            let x_max = a.x.max(b.x);
            a.y >= r.y && a.y < r.y + r.h && x_max >= r.x && x_min < r.x + r.w
        } else {
            // Non-axis-aligned (shouldn't happen in our routing)
            false
        }
    })
}

/// Route a corridor as a Z-shape when L-shape or straight routing would pass
/// through another room. Picks the appropriate Z-shape variant based on the
/// exit direction.
///
/// For straight corridors (where start and end share an axis), the Z-shape deviates
/// perpendicular to the corridor direction. A generous search range (±20 tiles)
/// ensures the transfer segment can find room-free space.
fn route_z_shape(
    start: Point,
    end: Point,
    exit_vertical: bool,
    rooms: &[PlacedSpace],
) -> Vec<Point> {
    if exit_vertical {
        // Exit is vertical → V→H→V (find safe horizontal transfer).
        // Expand range if start.x == end.x (straight vertical corridor needs x-deviation).
        let (x_lo, x_hi) = if start.x == end.x {
            (start.x - 20, start.x + 20)
        } else {
            (start.x.min(end.x), start.x.max(end.x))
        };
        let mid_y = find_safe_transfer_y_range(start.y, end.y, x_lo, x_hi, rooms);
        vec![
            start,
            Point {
                x: start.x,
                y: mid_y,
            },
            Point { x: end.x, y: mid_y },
            end,
        ]
    } else {
        // Exit is horizontal → H→V→H (find safe vertical transfer).
        // Expand range if start.y == end.y (straight horizontal corridor needs y-deviation).
        let (y_lo, y_hi) = if start.y == end.y {
            (start.y - 20, start.y + 20)
        } else {
            (start.y.min(end.y), start.y.max(end.y))
        };
        let mid_x = find_safe_transfer_x_range(start.x, end.x, y_lo, y_hi, rooms);
        vec![
            start,
            Point {
                x: mid_x,
                y: start.y,
            },
            Point { x: mid_x, y: end.y },
            end,
        ]
    }
}

/// Find a safe x-coordinate for a vertical transfer segment.
/// Unlike [`find_safe_transfer_x`], this searches outside the x_from..x_to
/// range (expanding outward) to handle straight corridors that need to deviate.
fn find_safe_transfer_x_range(
    x_from: i32,
    x_to: i32,
    y_min: i32,
    y_max: i32,
    rooms: &[PlacedSpace],
) -> i32 {
    let mid = (x_from + x_to) / 2;
    if !intersects_any_room_x(mid, y_min, y_max, rooms) {
        return mid;
    }

    // Search outward from midpoint (beyond the original range if needed).
    for offset in 1..=40 {
        let try_left = mid - offset;
        if !intersects_any_room_x(try_left, y_min, y_max, rooms) {
            return try_left;
        }
        let try_right = mid + offset;
        if !intersects_any_room_x(try_right, y_min, y_max, rooms) {
            return try_right;
        }
    }

    // Fallback: midpoint (will likely still conflict but connects).
    mid
}

/// Find a safe y-coordinate for a horizontal transfer segment.
/// Unlike [`find_safe_transfer_y`], this searches outside the y_from..y_to
/// range (expanding outward) to handle straight corridors that need to deviate.
fn find_safe_transfer_y_range(
    y_from: i32,
    y_to: i32,
    x_min: i32,
    x_max: i32,
    rooms: &[PlacedSpace],
) -> i32 {
    let mid = (y_from + y_to) / 2;
    if !intersects_any_room_y(mid, x_min, x_max, rooms) {
        return mid;
    }

    // Search outward from midpoint (beyond the original range if needed).
    for offset in 1..=40 {
        let try_up = mid - offset;
        if !intersects_any_room_y(try_up, x_min, x_max, rooms) {
            return try_up;
        }
        let try_down = mid + offset;
        if !intersects_any_room_y(try_down, x_min, x_max, rooms) {
            return try_down;
        }
    }

    // Fallback: midpoint.
    mid
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::graph::{EdgeRole, NodeRole, ScenarioNodeId};
    use crate::intent::map_intent::LocationKind;
    use crate::spatial::plan::{
        AtomicSpace, RealizationStyle, SizeHint, SpaceArchetype, SpaceId, SpaceKind, SpaceLink,
        SpaceSpec, SpatialPlan,
    };

    // ── Helpers ──────────────────────────────────────────────────────────

    fn make_spec(id: u32, role: NodeRole) -> SpaceSpec {
        SpaceSpec {
            id: SpaceId(id),
            origin: ScenarioNodeId(id),
            kind: SpaceKind::Atomic(AtomicSpace {
                width: 7,
                height: 7,
            }),
            role,
            archetype: Some(SpaceArchetype::Chamber),
            size_hint: SizeHint::Medium,
            style: RealizationStyle::RoomLike,
            structural_tags: vec![],
            atmosphere_tags: vec![],
            motifs: vec![],
            label: None,
        }
    }

    fn make_placed(id: u32, rect: Rect) -> PlacedSpace {
        PlacedSpace {
            space_id: SpaceId(id),
            rect,
            footprint: Footprint::Rect(rect),
            style: RealizationStyle::RoomLike,
            label: None,
        }
    }

    /// Helper: build a minimal SpatialPlan with two spaces linked together.
    fn two_space_plan() -> SpatialPlan {
        SpatialPlan {
            spaces: vec![
                make_spec(0, NodeRole::Entry),
                make_spec(1, NodeRole::Branch),
            ],
            links: vec![SpaceLink {
                from: SpaceId(0),
                to: SpaceId(1),
                role: EdgeRole::Traversal,
                tags: vec![],
            }],
            constraints: vec![],
            location_kind: LocationKind::Dungeon,
        }
    }

    // ── Basic routing (preserved from original) ─────────────────────────

    #[test]
    fn horizontal_aligned_produces_straight_corridor() {
        let spatial = two_space_plan();
        let placed = vec![
            make_placed(
                0,
                Rect {
                    x: 0,
                    y: 0,
                    w: 7,
                    h: 7,
                },
            ),
            make_placed(
                1,
                Rect {
                    x: 12,
                    y: 0,
                    w: 7,
                    h: 7,
                },
            ),
        ];

        let links = ZShapeRouter.route(&spatial, &placed);

        assert_eq!(links.len(), 1);
        let corridor = &links[0];
        assert_eq!(corridor.points.len(), 2);
        assert_eq!(corridor.points[0].y, corridor.points[1].y);
    }

    #[test]
    fn vertical_aligned_produces_straight_corridor() {
        let spatial = two_space_plan();
        let placed = vec![
            make_placed(
                0,
                Rect {
                    x: 0,
                    y: 0,
                    w: 7,
                    h: 7,
                },
            ),
            make_placed(
                1,
                Rect {
                    x: 0,
                    y: 12,
                    w: 7,
                    h: 7,
                },
            ),
        ];

        let links = ZShapeRouter.route(&spatial, &placed);

        assert_eq!(links.len(), 1);
        let corridor = &links[0];
        assert_eq!(corridor.points.len(), 2);
        assert_eq!(corridor.points[0].x, corridor.points[1].x);
    }

    #[test]
    fn non_aligned_produces_z_shaped_corridor() {
        let spatial = two_space_plan();
        let placed = vec![
            make_placed(
                0,
                Rect {
                    x: 0,
                    y: 0,
                    w: 7,
                    h: 7,
                },
            ),
            make_placed(
                1,
                Rect {
                    x: 12,
                    y: 12,
                    w: 7,
                    h: 7,
                },
            ),
        ];

        let links = ZShapeRouter.route(&spatial, &placed);

        assert_eq!(links.len(), 1);
        let corridor = &links[0];
        assert!(
            corridor.points.len() >= 3 && corridor.points.len() <= 4,
            "Expected 3 or 4 points for Z-shape, got {}",
            corridor.points.len()
        );
    }

    // ── Merging tests ───────────────────────────────────────────────────

    #[test]
    fn mergeable_links_same_face_share_exit_point() {
        // Hub (9×9) with two OptionalTraversal links going south to two rooms.
        let spatial = SpatialPlan {
            spaces: vec![
                make_spec(0, NodeRole::Hub),
                make_spec(1, NodeRole::Branch),
                make_spec(2, NodeRole::Branch),
            ],
            links: vec![
                SpaceLink {
                    from: SpaceId(0),
                    to: SpaceId(1),
                    role: EdgeRole::OptionalTraversal,
                    tags: vec![],
                },
                SpaceLink {
                    from: SpaceId(0),
                    to: SpaceId(2),
                    role: EdgeRole::OptionalTraversal,
                    tags: vec![],
                },
            ],
            constraints: vec![],
            location_kind: LocationKind::Dungeon,
        };

        let placed = vec![
            make_placed(
                0,
                Rect {
                    x: 0,
                    y: 0,
                    w: 9,
                    h: 9,
                },
            ),
            make_placed(
                1,
                Rect {
                    x: -3,
                    y: 20,
                    w: 7,
                    h: 7,
                },
            ),
            make_placed(
                2,
                Rect {
                    x: 3,
                    y: 20,
                    w: 7,
                    h: 7,
                },
            ),
        ];

        let links = ZShapeRouter.route(&spatial, &placed);

        assert_eq!(links.len(), 2);
        // Both corridors share the same exit point (one door on hub's south face).
        assert_eq!(links[0].points[0], links[1].points[0]);
        // But they end at different rooms.
        assert_ne!(links[0].points.last(), links[1].points.last());
    }

    #[test]
    fn restricted_link_gets_separate_exit_from_mergeable() {
        // Hub with one Traversal link and one RestrictedTraversal link, both south.
        let spatial = SpatialPlan {
            spaces: vec![
                make_spec(0, NodeRole::Hub),
                make_spec(1, NodeRole::Branch),
                make_spec(2, NodeRole::Gate),
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
                    role: EdgeRole::RestrictedTraversal,
                    tags: vec![],
                },
            ],
            constraints: vec![],
            location_kind: LocationKind::Dungeon,
        };

        let placed = vec![
            make_placed(
                0,
                Rect {
                    x: 0,
                    y: 0,
                    w: 9,
                    h: 9,
                },
            ),
            make_placed(
                1,
                Rect {
                    x: -3,
                    y: 20,
                    w: 7,
                    h: 7,
                },
            ),
            make_placed(
                2,
                Rect {
                    x: 3,
                    y: 20,
                    w: 7,
                    h: 7,
                },
            ),
        ];

        let links = ZShapeRouter.route(&spatial, &placed);

        assert_eq!(links.len(), 2);
        // The two corridors must exit from different points.
        assert_ne!(links[0].points[0], links[1].points[0]);
        // One is Normal door, the other LockedDoor.
        assert_eq!(links[0].kind, LinkKind::Normal);
        assert_eq!(links[1].kind, LinkKind::Restricted);
    }

    #[test]
    fn single_isolated_link_on_face_uses_center() {
        // One restricted link, no other links on the same face.
        let spatial = SpatialPlan {
            spaces: vec![make_spec(0, NodeRole::Gate), make_spec(1, NodeRole::Goal)],
            links: vec![SpaceLink {
                from: SpaceId(0),
                to: SpaceId(1),
                role: EdgeRole::RestrictedTraversal,
                tags: vec![],
            }],
            constraints: vec![],
            location_kind: LocationKind::Dungeon,
        };

        let placed = vec![
            make_placed(
                0,
                Rect {
                    x: 0,
                    y: 0,
                    w: 7,
                    h: 7,
                },
            ),
            make_placed(
                1,
                Rect {
                    x: 12,
                    y: 0,
                    w: 7,
                    h: 7,
                },
            ),
        ];

        let links = ZShapeRouter.route(&spatial, &placed);

        assert_eq!(links.len(), 1);
        // Exit should be at the center of the east face interior.
        // East face: fixed=6, interior y 1..5, center=3.
        assert_eq!(links[0].points[0], Point { x: 6, y: 3 });
        assert_eq!(links[0].kind, LinkKind::Restricted);
    }

    #[test]
    fn mixed_traversal_and_optional_merge_together() {
        // One Traversal and one OptionalTraversal going east — both mergeable.
        let spatial = SpatialPlan {
            spaces: vec![
                make_spec(0, NodeRole::Hub),
                make_spec(1, NodeRole::Branch),
                make_spec(2, NodeRole::Reward),
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
                    role: EdgeRole::OptionalTraversal,
                    tags: vec![],
                },
            ],
            constraints: vec![],
            location_kind: LocationKind::Dungeon,
        };

        let placed = vec![
            make_placed(
                0,
                Rect {
                    x: 0,
                    y: 0,
                    w: 9,
                    h: 9,
                },
            ),
            make_placed(
                1,
                Rect {
                    x: 20,
                    y: -3,
                    w: 7,
                    h: 7,
                },
            ),
            make_placed(
                2,
                Rect {
                    x: 20,
                    y: 5,
                    w: 7,
                    h: 7,
                },
            ),
        ];

        let links = ZShapeRouter.route(&spatial, &placed);

        assert_eq!(links.len(), 2);
        // Both exit from the same point (merged on east face).
        assert_eq!(links[0].points[0], links[1].points[0]);
    }

    #[test]
    fn face_determination_uses_dominant_axis() {
        // Room far to east and slightly south → east face, not south.
        let spatial = two_space_plan();
        let placed = vec![
            make_placed(
                0,
                Rect {
                    x: 0,
                    y: 0,
                    w: 7,
                    h: 7,
                },
            ),
            make_placed(
                1,
                Rect {
                    x: 20,
                    y: 2,
                    w: 7,
                    h: 7,
                },
            ),
        ];

        let links = ZShapeRouter.route(&spatial, &placed);
        let corridor = &links[0];

        // Exit should be on the east face (x = 6), not south.
        assert_eq!(corridor.points[0].x, 6);
    }

    // ── Collision-avoidance tests ─────────────────────────────────────────

    #[test]
    fn exit_face_switches_when_path_crosses_multiple_rooms() {
        // Room 0 at the bottom, room 1 at the top-right, rooms 2+3 stacked between them.
        // Natural exit from 0→1 is North (|dy| >> dx), but the vertical path
        // crosses rooms 2 and 3, so it should switch to East.
        let spatial = SpatialPlan {
            spaces: vec![
                make_spec(0, NodeRole::Gate),
                make_spec(1, NodeRole::Reward),
                make_spec(2, NodeRole::Branch),
                make_spec(3, NodeRole::Branch),
            ],
            links: vec![SpaceLink {
                from: SpaceId(0),
                to: SpaceId(1),
                role: EdgeRole::RestrictedTraversal,
                tags: vec![],
            }],
            constraints: vec![],
            location_kind: LocationKind::Dungeon,
        };

        // Room 0 at (20, 50), room 1 at (30, 2), blocking rooms 2+3 stacked between.
        let placed = vec![
            make_placed(
                0,
                Rect {
                    x: 20,
                    y: 50,
                    w: 7,
                    h: 7,
                },
            ),
            make_placed(
                1,
                Rect {
                    x: 30,
                    y: 2,
                    w: 7,
                    h: 7,
                },
            ),
            make_placed(
                2,
                Rect {
                    x: 20,
                    y: 20,
                    w: 7,
                    h: 7,
                },
            ),
            make_placed(
                3,
                Rect {
                    x: 20,
                    y: 35,
                    w: 7,
                    h: 7,
                },
            ),
        ];

        let links = ZShapeRouter.route(&spatial, &placed);
        let corridor = &links[0];

        // Should exit East (x = room0.x + w - 1 = 26), not North (y = room0.y = 50).
        assert_eq!(
            corridor.points[0].x, 26,
            "Expected East exit at x=26, got start={:?}",
            corridor.points[0]
        );
    }

    #[test]
    fn single_blocking_room_keeps_natural_face() {
        // Same layout but only 1 blocking room → should keep natural North exit.
        let spatial = SpatialPlan {
            spaces: vec![
                make_spec(0, NodeRole::Gate),
                make_spec(1, NodeRole::Reward),
                make_spec(2, NodeRole::Branch),
            ],
            links: vec![SpaceLink {
                from: SpaceId(0),
                to: SpaceId(1),
                role: EdgeRole::RestrictedTraversal,
                tags: vec![],
            }],
            constraints: vec![],
            location_kind: LocationKind::Dungeon,
        };

        let placed = vec![
            make_placed(
                0,
                Rect {
                    x: 20,
                    y: 50,
                    w: 7,
                    h: 7,
                },
            ),
            make_placed(
                1,
                Rect {
                    x: 30,
                    y: 2,
                    w: 7,
                    h: 7,
                },
            ),
            make_placed(
                2,
                Rect {
                    x: 20,
                    y: 30,
                    w: 7,
                    h: 7,
                },
            ),
        ];

        let links = ZShapeRouter.route(&spatial, &placed);
        let corridor = &links[0];

        // Only 1 blocking room → keep natural North exit (y = 50).
        assert_eq!(
            corridor.points[0].y, 50,
            "Expected North exit at y=50, got start={:?}",
            corridor.points[0]
        );
    }
}
