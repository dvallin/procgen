use procgen::demo::crypt::{CryptIntentBuilder, build_crypt_situation};
use procgen::demo::tavern::{TavernIntentBuilder, build_tavern_situation};
use procgen::geometry::geom::Point;
use procgen::geometry::planner::{GeometryPlanner, SimpleGeometryPlanner};
use procgen::intent::builder::IntentBuilder;
use procgen::intent::map_intent::MapIntent;
use procgen::spatial::planner::{SimpleSpatialPlanner, SpatialPlanner};
use procgen::tile::map::Tile;
use procgen::tile::rasterize::{Rasterizer, SimpleRasterizer};

/// Helper: build the crypt MapIntent via the Phase 2 path.
fn build_crypt_intent() -> MapIntent {
    let situation = build_crypt_situation();
    CryptIntentBuilder.build(&situation).unwrap()
}

/// Helper: run the full pipeline and return the tile map.
fn run_full_pipeline() -> procgen::tile::map::TileMap {
    let intent = build_crypt_intent();
    let spatial_plan = SimpleSpatialPlanner.plan(&intent).unwrap();
    let geometry_plan = SimpleGeometryPlanner::default()
        .plan(&spatial_plan)
        .unwrap();
    SimpleRasterizer.rasterize(&geometry_plan).unwrap()
}

/// Helper: run up to the spatial plan stage.
fn run_spatial_plan() -> procgen::spatial::plan::SpatialPlan {
    let intent = build_crypt_intent();
    SimpleSpatialPlanner.plan(&intent).unwrap()
}

/// Helper: run up to the geometry plan stage.
fn run_geometry_plan() -> procgen::geometry::geom::GeometryPlan {
    let intent = build_crypt_intent();
    let spatial_plan = SimpleSpatialPlanner.plan(&intent).unwrap();
    SimpleGeometryPlanner::default()
        .plan(&spatial_plan)
        .unwrap()
}

#[test]
fn crypt_pipeline_produces_valid_map() {
    let map = run_full_pipeline();

    // The tile map is non-empty
    assert!(map.width > 0, "map width should be > 0");
    assert!(map.height > 0, "map height should be > 0");

    // There is at least one Floor tile
    assert!(
        map.tiles.iter().any(|t| *t == Tile::Floor),
        "map should contain at least one Floor tile"
    );

    // There is at least one Door tile
    assert!(
        map.tiles.iter().any(|t| *t == Tile::Door),
        "map should contain at least one Door tile"
    );

    // There is at least one LockedDoor tile (from the restricted traversal)
    assert!(
        map.tiles.iter().any(|t| *t == Tile::LockedDoor),
        "map should contain at least one LockedDoor tile"
    );

    // There is at least one Wall tile
    assert!(
        map.tiles.iter().any(|t| *t == Tile::Wall),
        "map should contain at least one Wall tile"
    );
}

#[test]
fn crypt_map_is_connected() {
    let map = run_full_pipeline();

    // Find the first walkable tile as the BFS start
    let start = (0..map.height as i32)
        .flat_map(|y| (0..map.width as i32).map(move |x| Point { x, y }))
        .find(|p| map.get(p.x, p.y).is_some_and(|t| t.is_walkable()))
        .expect("map should have at least one walkable tile");

    // BFS flood fill using the TileMap helper
    let reached = map.flood_fill(start, |t| t.is_walkable());

    // Count all walkable tiles
    let total_walkable = map.tiles.iter().filter(|t| t.is_walkable()).count();

    assert_eq!(
        reached.len(),
        total_walkable,
        "all walkable tiles should be reachable from any walkable tile (map must be connected); \
         reached {} out of {} walkable tiles",
        reached.len(),
        total_walkable
    );
}

#[test]
fn crypt_pipeline_space_count_matches() {
    let spatial_plan = run_spatial_plan();

    // The crypt demo has 6 nodes and 5 edges
    assert_eq!(
        spatial_plan.spaces.len(),
        6,
        "spatial plan should have exactly 6 spaces (one per scenario node)"
    );
    assert_eq!(
        spatial_plan.links.len(),
        5,
        "spatial plan should have exactly 5 links (one per scenario edge)"
    );
}

#[test]
fn crypt_geometry_no_overlapping_rooms() {
    let geometry_plan = run_geometry_plan();

    let spaces = &geometry_plan.spaces;
    for i in 0..spaces.len() {
        for j in (i + 1)..spaces.len() {
            let a = &spaces[i];
            let b = &spaces[j];
            assert!(
                !a.rect.overlaps(&b.rect),
                "rooms {:?} (label: {:?}, rect: {:?}) and {:?} (label: {:?}, rect: {:?}) overlap",
                a.space_id,
                a.label,
                a.rect,
                b.space_id,
                b.label,
                b.rect,
            );
        }
    }
}

#[test]
fn crypt_intent_has_correct_metadata() {
    let intent = build_crypt_intent();

    assert_eq!(
        intent.location_kind,
        procgen::intent::map_intent::LocationKind::Dungeon
    );
    assert_eq!(intent.scale, procgen::intent::map_intent::MapScale::Small);
    assert!(
        !intent.tags.is_empty(),
        "intent should carry situation tags"
    );
    assert!(!intent.motifs.is_empty(), "intent should have motifs");
    assert_eq!(
        intent.structural_graph.nodes.len(),
        6,
        "structural graph should have 6 nodes"
    );
    assert_eq!(
        intent.structural_graph.edges.len(),
        5,
        "structural graph should have 5 edges"
    );
}

// ============================================================
// Tavern Cellar tests
// ============================================================

/// Helper: build the tavern MapIntent.
fn build_tavern_intent() -> MapIntent {
    let situation = build_tavern_situation();
    TavernIntentBuilder.build(&situation).unwrap()
}

/// Helper: run the full tavern pipeline.
fn run_tavern_pipeline() -> procgen::tile::map::TileMap {
    let intent = build_tavern_intent();
    let spatial_plan = SimpleSpatialPlanner.plan(&intent).unwrap();
    let geometry_plan = SimpleGeometryPlanner::default()
        .plan(&spatial_plan)
        .unwrap();
    SimpleRasterizer.rasterize(&geometry_plan).unwrap()
}

/// Helper: tavern spatial plan.
fn run_tavern_spatial_plan() -> procgen::spatial::plan::SpatialPlan {
    let intent = build_tavern_intent();
    SimpleSpatialPlanner.plan(&intent).unwrap()
}

/// Helper: tavern geometry plan.
fn run_tavern_geometry_plan() -> procgen::geometry::geom::GeometryPlan {
    let intent = build_tavern_intent();
    let spatial_plan = SimpleSpatialPlanner.plan(&intent).unwrap();
    SimpleGeometryPlanner::default()
        .plan(&spatial_plan)
        .unwrap()
}

#[test]
fn tavern_pipeline_produces_valid_map() {
    let map = run_tavern_pipeline();

    assert!(map.width > 0);
    assert!(map.height > 0);
    assert!(map.tiles.iter().any(|t| *t == Tile::Floor));
    assert!(map.tiles.iter().any(|t| *t == Tile::Door));
    assert!(map.tiles.iter().any(|t| *t == Tile::Wall));
    // Tavern has a restricted traversal (cold_room → tunnel), so LockedDoor expected
    assert!(
        map.tiles.iter().any(|t| *t == Tile::LockedDoor),
        "tavern should have a LockedDoor (smuggler's tunnel gate)"
    );
}

#[test]
fn tavern_map_is_connected() {
    let map = run_tavern_pipeline();

    let start = (0..map.height as i32)
        .flat_map(|y| (0..map.width as i32).map(move |x| Point { x, y }))
        .find(|p| map.get(p.x, p.y).is_some_and(|t| t.is_walkable()))
        .expect("map should have at least one walkable tile");

    let reached = map.flood_fill(start, |t| t.is_walkable());
    let total_walkable = map.tiles.iter().filter(|t| t.is_walkable()).count();

    assert_eq!(
        reached.len(),
        total_walkable,
        "tavern map must be fully connected; reached {} out of {} walkable tiles",
        reached.len(),
        total_walkable
    );
}

#[test]
fn tavern_pipeline_space_count_matches() {
    let spatial_plan = run_tavern_spatial_plan();

    // Tavern has 6 nodes and 5 edges
    assert_eq!(spatial_plan.spaces.len(), 6);
    assert_eq!(spatial_plan.links.len(), 5);
}

#[test]
fn tavern_geometry_no_overlapping_rooms() {
    let geometry_plan = run_tavern_geometry_plan();

    let spaces = &geometry_plan.spaces;
    for i in 0..spaces.len() {
        for j in (i + 1)..spaces.len() {
            let a = &spaces[i];
            let b = &spaces[j];
            assert!(
                !a.rect.overlaps(&b.rect),
                "tavern rooms {:?} ({:?}) and {:?} ({:?}) overlap",
                a.space_id,
                a.rect,
                b.space_id,
                b.rect,
            );
        }
    }
}

#[test]
fn tavern_intent_has_correct_metadata() {
    let intent = build_tavern_intent();

    assert_eq!(
        intent.location_kind,
        procgen::intent::map_intent::LocationKind::Building
    );
    assert_eq!(intent.scale, procgen::intent::map_intent::MapScale::Small);
    assert!(!intent.tags.is_empty());
    assert!(!intent.motifs.is_empty());
    assert_eq!(intent.structural_graph.nodes.len(), 6);
    assert_eq!(intent.structural_graph.edges.len(), 5);
}

#[test]
fn tavern_spatial_plan_has_constraints() {
    let spatial_plan = run_tavern_spatial_plan();

    // Should have at least GatedBy + PreferCentral + PreferPerimeter constraints
    assert!(
        !spatial_plan.constraints.is_empty(),
        "tavern spatial plan should derive constraints"
    );
}

// ============================================================
// Shared door-quality tests
// ============================================================

/// Doors must have walkable tiles on at least 2 cardinal sides.
fn assert_doors_connect_two_sides(map: &procgen::tile::map::TileMap, label: &str) {
    for y in 0..map.height as i32 {
        for x in 0..map.width as i32 {
            let tile = map.get(x, y).unwrap();
            if tile != Tile::Door && tile != Tile::LockedDoor {
                continue;
            }
            let walkable_count = Point { x, y }
                .cardinals()
                .iter()
                .filter(|n| map.get(n.x, n.y).is_some_and(|t| t.is_walkable()))
                .count();
            assert!(
                walkable_count >= 2,
                "{label}: door at ({x}, {y}) only has {walkable_count} walkable cardinal \
                 neighbor(s) — it should connect two areas"
            );
        }
    }
}

#[test]
fn crypt_doors_connect_two_sides() {
    let map = run_full_pipeline();
    assert_doors_connect_two_sides(&map, "crypt");
}

#[test]
fn tavern_doors_connect_two_sides() {
    let map = run_tavern_pipeline();
    assert_doors_connect_two_sides(&map, "tavern");
}
