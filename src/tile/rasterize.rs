use crate::geometry::geom::*;
use crate::tile::map::*;

#[derive(Debug)]
pub enum RasterizeError {
    EmptyLayout,
}

impl std::fmt::Display for RasterizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyLayout => write!(f, "cannot rasterize empty layout"),
        }
    }
}

impl std::error::Error for RasterizeError {}

pub trait Rasterizer {
    fn rasterize(&self, layout: &GeometryPlan) -> Result<TileMap, RasterizeError>;
}

#[derive(Default)]
pub struct SimpleRasterizer;

impl Rasterizer for SimpleRasterizer {
    fn rasterize(&self, layout: &GeometryPlan) -> Result<TileMap, RasterizeError> {
        if layout.spaces.is_empty() {
            return Err(RasterizeError::EmptyLayout);
        }

        let mut max_x = 0;
        let mut max_y = 0;

        for f in &layout.spaces {
            max_x = max_x.max(f.rect.x + f.rect.w + 2);
            max_y = max_y.max(f.rect.y + f.rect.h + 2);
        }
        for link in &layout.links {
            for p in &link.points {
                max_x = max_x.max(p.x + 2);
                max_y = max_y.max(p.y + 2);
            }
        }

        let mut map = TileMap::new((max_x + 1) as u32, (max_y + 1) as u32);

        for f in &layout.spaces {
            fill_room(&mut map, &f.footprint);
        }

        for link in &layout.links {
            carve_link(&mut map, link);
        }

        infer_walls(&mut map);

        Ok(map)
    }
}

fn fill_room(map: &mut TileMap, footprint: &Footprint) {
    match footprint {
        Footprint::Rect(rect) => {
            for y in rect.y + 1..rect.y + rect.h - 1 {
                for x in rect.x + 1..rect.x + rect.w - 1 {
                    map.set(x, y, Tile::Floor);
                }
            }
        }
        Footprint::Cells(cells) => {
            for p in cells {
                map.set(p.x, p.y, Tile::Floor);
            }
        }
        Footprint::Composite(parts) => {
            for part in parts {
                fill_room(map, part);
            }
        }
    }
}

fn carve_link(map: &mut TileMap, link: &PlacedLink) {
    for window in link.points.windows(2) {
        let a = window[0];
        let b = window[1];

        if a.x == b.x {
            let (from, to) = if a.y <= b.y { (a.y, b.y) } else { (b.y, a.y) };
            for y in from..=to {
                map.set(a.x, y, Tile::Floor);
            }
        } else if a.y == b.y {
            let (from, to) = if a.x <= b.x { (a.x, b.x) } else { (b.x, a.x) };
            for x in from..=to {
                map.set(x, a.y, Tile::Floor);
            }
        }
    }

    if let Some(start) = link.points.first() {
        map.set(
            start.x,
            start.y,
            match link.kind {
                LinkKind::Restricted => Tile::LockedDoor,
                _ => Tile::Door,
            },
        );
    }

    if let Some(end) = link.points.last() {
        map.set(
            end.x,
            end.y,
            match link.kind {
                LinkKind::Restricted => Tile::LockedDoor,
                _ => Tile::Door,
            },
        );
    }
}

fn infer_walls(map: &mut TileMap) {
    let original = map.tiles.clone();

    for y in 0..map.height as i32 {
        for x in 0..map.width as i32 {
            let idx = (y as u32 * map.width + x as u32) as usize;
            if original[idx] != Tile::Void {
                continue;
            }

            let pos = Point { x, y };
            let touches_floorish = pos.neighbors().iter().any(|n| {
                if !map.in_bounds(n.x, n.y) {
                    return false;
                }
                let nidx = (n.y as u32 * map.width + n.x as u32) as usize;
                original[nidx].is_walkable()
            });

            if touches_floorish {
                map.set(x, y, Tile::Wall);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::geom::{
        Footprint, GeometryPlan, LinkKind, PlacedLink, PlacedSpace, Point, Rect,
    };
    use crate::spatial::plan::{RealizationStyle, SpaceId};
    use crate::tile::map::Tile;
    use proptest::prelude::*;

    fn arb_rect() -> impl Strategy<Value = Rect> {
        (0i32..30, 0i32..30, 3i32..=12, 3i32..=12).prop_map(|(x, y, w, h)| Rect { x, y, w, h })
    }

    fn arb_link_kind() -> impl Strategy<Value = LinkKind> {
        prop_oneof![
            Just(LinkKind::Normal),
            Just(LinkKind::Restricted),
            Just(LinkKind::Optional),
        ]
    }

    fn arb_placed_link() -> impl Strategy<Value = PlacedLink> {
        // Generate a horizontal or vertical 2-point segment
        let horizontal = (0i32..60, 0i32..60, 1i32..20)
            .prop_map(|(x, y, len)| vec![Point { x, y }, Point { x: x + len, y }]);
        let vertical = (0i32..60, 0i32..60, 1i32..20)
            .prop_map(|(x, y, len)| vec![Point { x, y }, Point { x, y: y + len }]);
        let points_strat = prop_oneof![horizontal, vertical];

        (arb_link_kind(), points_strat).prop_map(|(kind, points)| PlacedLink { kind, points })
    }

    fn arb_geometry_plan() -> impl Strategy<Value = GeometryPlan> {
        let spaces_strat = (1usize..=5).prop_flat_map(|count| {
            proptest::collection::vec(arb_rect(), count).prop_map(|rects| {
                rects
                    .into_iter()
                    .enumerate()
                    .map(|(i, rect)| PlacedSpace {
                        space_id: SpaceId(i as u32),
                        rect,
                        footprint: Footprint::Rect(rect),
                        style: RealizationStyle::RoomLike,
                        label: None,
                    })
                    .collect::<Vec<_>>()
            })
        });

        let links_strat = proptest::collection::vec(arb_placed_link(), 0..=4);

        (spaces_strat, links_strat).prop_map(|(spaces, links)| GeometryPlan { spaces, links })
    }

    proptest! {
        /// Every floor tile must have only Wall, Floor, Door, or LockedDoor as
        /// its cardinal+diagonal neighbors (no Void). This validates the wall
        /// inference guarantee.
        #[test]
        fn no_floor_adjacent_to_void(plan in arb_geometry_plan()) {
            let rasterizer = SimpleRasterizer;
            let map = rasterizer.rasterize(&plan).unwrap();

            for y in 0..map.height as i32 {
                for x in 0..map.width as i32 {
                    if map.get(x, y) != Some(Tile::Floor) {
                        continue;
                    }
                    let pos = Point { x, y };
                    for n in pos.neighbors() {
                        if !map.in_bounds(n.x, n.y) {
                            continue;
                        }
                        let neighbor = map.get(n.x, n.y).unwrap();
                        prop_assert!(
                            neighbor != Tile::Void,
                            "Floor at ({}, {}) has Void neighbor at ({}, {})",
                            x, y, n.x, n.y
                        );
                    }
                }
            }
        }

        /// For each space's rect, all tiles strictly inside the rect should be
        /// Floor (or Door/LockedDoor if a link carves through).
        #[test]
        fn room_interiors_are_floor(plan in arb_geometry_plan()) {
            let rasterizer = SimpleRasterizer;
            let map = rasterizer.rasterize(&plan).unwrap();

            for space in &plan.spaces {
                let r = &space.rect;
                for y in (r.y + 1)..(r.y + r.h - 1) {
                    for x in (r.x + 1)..(r.x + r.w - 1) {
                        if let Some(tile) = map.get(x, y) {
                            prop_assert!(
                                tile.is_walkable(),
                                "Interior tile at ({}, {}) is {:?}, expected walkable",
                                x, y, tile
                            );
                        }
                    }
                }
            }
        }

        /// Every Door/LockedDoor tile has at least one cardinal neighbor
        /// that is walkable. (The stronger 2-neighbor property is tested
        /// in integration tests against real pipeline output.)
        #[test]
        fn doors_have_walkable_neighbor(plan in arb_geometry_plan()) {
            let rasterizer = SimpleRasterizer;
            let map = rasterizer.rasterize(&plan).unwrap();

            for y in 0..map.height as i32 {
                for x in 0..map.width as i32 {
                    let tile = map.get(x, y).unwrap();
                    if tile != Tile::Door && tile != Tile::LockedDoor {
                        continue;
                    }
                    let pos = Point { x, y };
                    let has_walkable = pos.cardinals().iter().any(|n| {
                        map.get(n.x, n.y).is_some_and(|t| t.is_walkable())
                    });
                    prop_assert!(
                        has_walkable,
                        "Door/LockedDoor at ({}, {}) has no walkable cardinal neighbor",
                        x, y
                    );
                }
            }
        }
    }
}
