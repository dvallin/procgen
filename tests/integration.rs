use procgen::demo::crypt::{CryptIntentBuilder, build_crypt_situation};
use procgen::demo::tavern::{TavernIntentBuilder, build_tavern_situation};
use procgen::geometry::geom::Point;
use procgen::geometry::planner::{GeometryPlanner, SimpleGeometryPlanner};
use procgen::intent::builder::IntentBuilder;
use procgen::intent::map_intent::MapIntent;
use procgen::spatial::planner::{SimpleSpatialPlanner, SpatialPlanner};
use procgen::tile::map::Tile;
use procgen::tile::rasterize::{Rasterizer, SimpleRasterizer};
use procgen::validate::Validator;

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
// Geometry validation integration tests
// ============================================================

#[test]
fn crypt_geometry_passes_validation() {
    let geometry_plan = run_geometry_plan();
    let validator = procgen::validate::geometry::GeometryValidator::default();
    let result = validator.validate(&geometry_plan);
    let errors: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.severity == procgen::validate::Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "crypt geometry should have no validation errors, got: {:?}",
        errors
    );
}

#[test]
fn tavern_geometry_passes_validation() {
    let geometry_plan = run_tavern_geometry_plan();
    let validator = procgen::validate::geometry::GeometryValidator::default();
    let result = validator.validate(&geometry_plan);
    let errors: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.severity == procgen::validate::Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "tavern geometry should have no validation errors, got: {:?}",
        errors
    );
}

#[test]
fn crypt_plan_with_validation_succeeds() {
    let intent = build_crypt_intent();
    let spatial_plan = SimpleSpatialPlanner.plan(&intent).unwrap();
    let result = SimpleGeometryPlanner::default().plan_with_validation(&spatial_plan);
    assert!(
        result.is_ok(),
        "crypt plan_with_validation should succeed: {:?}",
        result.err()
    );
}

#[test]
fn tavern_plan_with_validation_succeeds() {
    let intent = build_tavern_intent();
    let spatial_plan = SimpleSpatialPlanner.plan(&intent).unwrap();
    let result = SimpleGeometryPlanner::default().plan_with_validation(&spatial_plan);
    assert!(
        result.is_ok(),
        "tavern plan_with_validation should succeed: {:?}",
        result.err()
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

// ============================================================
// Phase 4.7: Stress tests — random SpatialPlans through full pipeline
// ============================================================

mod stress_tests {
    use super::*;
    use procgen::geometry::planner::SimpleGeometryPlanner;
    use procgen::intent::graph::{EdgeRole, NodeRole, ScenarioNodeId};
    use procgen::spatial::plan::{
        AtomicSpace, RealizationStyle, SizeHint, SpaceArchetype, SpaceId, SpaceKind, SpaceLink,
        SpaceSpec, SpatialConstraint, SpatialPlan,
    };
    use procgen::tile::rasterize::{Rasterizer, SimpleRasterizer};
    use procgen::validate::geometry::GeometryValidator;
    use proptest::prelude::*;

    /// Strategy for a random NodeRole.
    fn arb_node_role() -> impl Strategy<Value = NodeRole> {
        prop_oneof![
            Just(NodeRole::Entry),
            Just(NodeRole::Hub),
            Just(NodeRole::Branch),
            Just(NodeRole::Goal),
            Just(NodeRole::Reward),
            Just(NodeRole::Gate),
            Just(NodeRole::Transition),
        ]
    }

    /// Strategy for a random EdgeRole.
    fn arb_edge_role() -> impl Strategy<Value = EdgeRole> {
        prop_oneof![
            Just(EdgeRole::Traversal),
            Just(EdgeRole::OptionalTraversal),
            Just(EdgeRole::RestrictedTraversal),
            Just(EdgeRole::SecretTraversal),
        ]
    }

    /// Strategy for a random SpaceArchetype.
    fn arb_archetype() -> impl Strategy<Value = SpaceArchetype> {
        prop_oneof![
            Just(SpaceArchetype::Hall),
            Just(SpaceArchetype::Chamber),
            Just(SpaceArchetype::Corridor),
            Just(SpaceArchetype::Vestibule),
            Just(SpaceArchetype::Vault),
            Just(SpaceArchetype::Shaft),
            Just(SpaceArchetype::Courtyard),
            Just(SpaceArchetype::Workshop),
        ]
    }

    /// Strategy for a random SizeHint.
    fn arb_size_hint() -> impl Strategy<Value = SizeHint> {
        prop_oneof![
            Just(SizeHint::Tiny),
            Just(SizeHint::Small),
            Just(SizeHint::Medium),
            Just(SizeHint::Large),
            Just(SizeHint::Grand),
        ]
    }

    /// Generate a random SpatialPlan with `n` spaces connected as a random tree.
    /// Ensures connectivity: each new space connects to a random existing space.
    fn arb_spatial_plan(
        min_spaces: usize,
        max_spaces: usize,
    ) -> impl Strategy<Value = SpatialPlan> {
        (min_spaces..=max_spaces)
            .prop_flat_map(|n| {
                let spaces_strat = proptest::collection::vec(
                    (
                        arb_node_role(),
                        arb_archetype(),
                        arb_size_hint(),
                        5i32..=11,
                        5i32..=9,
                    ),
                    n,
                );
                // For n spaces, we need n-1 edges to form a tree.
                // Each edge i (for i in 1..n) connects to a random node in 0..i.
                let edges_parents: Vec<_> = (1..n).collect();
                let edge_parents_strat = edges_parents
                    .into_iter()
                    .map(|i| (0..i, arb_edge_role()))
                    .collect::<Vec<_>>();
                let edge_strat = edge_parents_strat;

                (spaces_strat, edge_strat)
            })
            .prop_map(|(space_params, edge_params)| {
                let n = space_params.len();

                // Build spaces
                let spaces: Vec<SpaceSpec> = space_params
                    .into_iter()
                    .enumerate()
                    .map(|(i, (role, archetype, _size_hint, w, h))| {
                        // First space is always Entry for a well-formed plan.
                        let actual_role = if i == 0 { NodeRole::Entry } else { role };
                        SpaceSpec {
                            id: SpaceId(i as u32),
                            origin: ScenarioNodeId(i as u32),
                            role: actual_role,
                            tags: vec![],
                            style: RealizationStyle::RoomLike,
                            kind: SpaceKind::Atomic(AtomicSpace {
                                width: w,
                                height: h,
                            }),
                            label: Some(format!("space_{}", i)),
                            archetype: Some(archetype),
                            size_hint: SizeHint::Medium,
                        }
                    })
                    .collect();

                // Build tree edges
                let links: Vec<SpaceLink> = edge_params
                    .into_iter()
                    .enumerate()
                    .map(|(i, (parent, edge_role))| SpaceLink {
                        from: SpaceId(parent as u32),
                        to: SpaceId((i + 1) as u32),
                        role: edge_role,
                        tags: vec![],
                    })
                    .collect();

                // Derive basic constraints: first hub gets PreferCentral,
                // entry gets PreferPerimeter.
                let mut constraints = Vec::new();
                for space in &spaces {
                    if space.role == NodeRole::Hub {
                        constraints.push(SpatialConstraint::PreferCentral { space: space.id });
                    }
                    if space.role == NodeRole::Entry {
                        constraints.push(SpatialConstraint::PreferPerimeter { space: space.id });
                    }
                }

                // Occasionally add MustBeSeparated between the last two spaces.
                if n >= 3 {
                    constraints.push(SpatialConstraint::MustBeSeparated {
                        a: SpaceId((n - 2) as u32),
                        b: SpaceId((n - 1) as u32),
                    });
                }

                SpatialPlan {
                    spaces,
                    links,
                    constraints,
                }
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Random SpatialPlans (3–10 spaces) should pass geometry validation
        /// after plan_with_validation.
        #[test]
        fn random_plan_passes_validation(spatial in arb_spatial_plan(3, 10)) {
            let planner = SimpleGeometryPlanner::default();
            let geometry = planner.plan_with_validation(&spatial);
            prop_assert!(
                geometry.is_ok(),
                "plan_with_validation failed for {}-space plan: {:?}",
                spatial.spaces.len(),
                geometry.err()
            );

            // Additionally check that the raw validator finds no errors.
            let plan = geometry.unwrap();
            let validator = GeometryValidator::default();
            let result = validator.validate(&plan);
            let errors: Vec<_> = result
                .issues
                .iter()
                .filter(|i| i.severity == procgen::validate::Severity::Error)
                .collect();
            prop_assert!(
                errors.is_empty(),
                "validator found errors on {}-space plan: {:?}",
                spatial.spaces.len(),
                errors
            );
        }

        /// Random SpatialPlans rasterized should produce connected walkable maps.
        #[test]
        fn random_plan_produces_connected_map(spatial in arb_spatial_plan(3, 8)) {
            let planner = SimpleGeometryPlanner::default();
            let geometry = planner.plan_with_validation(&spatial);
            prop_assert!(geometry.is_ok(), "geometry planning failed: {:?}", geometry.err());

            let rasterizer = SimpleRasterizer;
            let map = rasterizer.rasterize(&geometry.unwrap());
            prop_assert!(map.is_ok(), "rasterization failed: {:?}", map.err());
            let map = map.unwrap();

            // Find the first walkable tile.
            let start = (0..map.height as i32)
                .flat_map(|y| (0..map.width as i32).map(move |x| Point { x, y }))
                .find(|p| map.get(p.x, p.y).is_some_and(|t| t.is_walkable()));

            if let Some(start) = start {
                let reached = map.flood_fill(start, |t| t.is_walkable());
                let total_walkable = map.tiles.iter().filter(|t| t.is_walkable()).count();

                prop_assert_eq!(
                    reached.len(),
                    total_walkable,
                    "map not connected: reached {} of {} walkable tiles for {}-space plan",
                    reached.len(),
                    total_walkable,
                    spatial.spaces.len()
                );
            }
        }

        /// Every rasterized room interior should be walkable.
        #[test]
        fn random_plan_room_interiors_are_walkable(spatial in arb_spatial_plan(3, 8)) {
            let planner = SimpleGeometryPlanner::default();
            let geometry = planner.plan_with_validation(&spatial);
            prop_assert!(geometry.is_ok());
            let geometry = geometry.unwrap();

            let rasterizer = SimpleRasterizer;
            let map = rasterizer.rasterize(&geometry);
            prop_assert!(map.is_ok());
            let map = map.unwrap();

            for space in &geometry.spaces {
                let r = &space.rect;
                for y in (r.y + 1)..(r.y + r.h - 1) {
                    for x in (r.x + 1)..(r.x + r.w - 1) {
                        if let Some(tile) = map.get(x, y) {
                            prop_assert!(
                                tile.is_walkable(),
                                "interior tile at ({}, {}) in space {:?} is {:?}",
                                x, y, space.space_id, tile
                            );
                        }
                    }
                }
            }
        }

        /// No floor tile should be adjacent to void (wall inference guarantee).
        #[test]
        fn random_plan_no_floor_adjacent_to_void(spatial in arb_spatial_plan(3, 8)) {
            let planner = SimpleGeometryPlanner::default();
            let geometry = planner.plan_with_validation(&spatial);
            prop_assert!(geometry.is_ok());

            let rasterizer = SimpleRasterizer;
            let map = rasterizer.rasterize(&geometry.unwrap());
            prop_assert!(map.is_ok());
            let map = map.unwrap();

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

        /// Doors should have at least 2 walkable cardinal neighbors.
        #[test]
        fn random_plan_doors_connect_two_sides(spatial in arb_spatial_plan(3, 8)) {
            let planner = SimpleGeometryPlanner::default();
            let geometry = planner.plan_with_validation(&spatial);
            prop_assert!(geometry.is_ok());

            let rasterizer = SimpleRasterizer;
            let map = rasterizer.rasterize(&geometry.unwrap());
            prop_assert!(map.is_ok());
            let map = map.unwrap();

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
                    prop_assert!(
                        walkable_count >= 2,
                        "Door at ({}, {}) has only {} walkable cardinal neighbors",
                        x, y, walkable_count
                    );
                }
            }
        }
    }
}
