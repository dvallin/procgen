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

// ============================================================
// Phase 5: Feature placement integration tests
// ============================================================

use procgen::feature::plan::FeatureKind;
use procgen::feature::planner::{FeaturePlanner, SimpleFeaturePlanner};
use procgen::validate::feature::{FeatureValidationInput, FeatureValidator};

use rand::SeedableRng;
use rand::rngs::StdRng;

/// Helper: run the crypt pipeline through feature placement.
fn run_crypt_features() -> (
    procgen::spatial::plan::SpatialPlan,
    procgen::geometry::geom::GeometryPlan,
    procgen::tile::map::TileMap,
    procgen::feature::plan::FeaturePlan,
) {
    let intent = build_crypt_intent();
    let spatial = SimpleSpatialPlanner.plan(&intent).unwrap();
    let geometry = SimpleGeometryPlanner::default()
        .plan_with_validation(&spatial)
        .unwrap();
    let map = SimpleRasterizer.rasterize(&geometry).unwrap();
    let mut rng = StdRng::seed_from_u64(42);
    let features = SimpleFeaturePlanner
        .plan(&spatial, &geometry, &map, &mut rng)
        .unwrap();
    (spatial, geometry, map, features)
}

/// Helper: run the tavern pipeline through feature placement.
fn run_tavern_features() -> (
    procgen::spatial::plan::SpatialPlan,
    procgen::geometry::geom::GeometryPlan,
    procgen::tile::map::TileMap,
    procgen::feature::plan::FeaturePlan,
) {
    let intent = build_tavern_intent();
    let spatial = SimpleSpatialPlanner.plan(&intent).unwrap();
    let geometry = SimpleGeometryPlanner::default()
        .plan_with_validation(&spatial)
        .unwrap();
    let map = SimpleRasterizer.rasterize(&geometry).unwrap();
    let mut rng = StdRng::seed_from_u64(42);
    let features = SimpleFeaturePlanner
        .plan(&spatial, &geometry, &map, &mut rng)
        .unwrap();
    (spatial, geometry, map, features)
}

#[test]
fn crypt_feature_plan_succeeds() {
    let (_, _, _, features) = run_crypt_features();
    assert!(
        !features.features.is_empty(),
        "crypt should have at least one feature placed"
    );
}

#[test]
fn tavern_feature_plan_succeeds() {
    let (_, _, _, features) = run_tavern_features();
    assert!(
        !features.features.is_empty(),
        "tavern should have at least one feature placed"
    );
}

#[test]
fn crypt_features_pass_validation() {
    let (spatial, geometry, map, features) = run_crypt_features();
    let result = FeatureValidator.validate(&FeatureValidationInput {
        features: &features,
        tiles: &map,
        geometry: &geometry,
        spatial: &spatial,
    });
    assert!(
        result.is_ok(),
        "crypt feature validation should pass, issues: {:?}",
        result.issues
    );
}

#[test]
fn tavern_features_pass_validation() {
    let (spatial, geometry, map, features) = run_tavern_features();
    let result = FeatureValidator.validate(&FeatureValidationInput {
        features: &features,
        tiles: &map,
        geometry: &geometry,
        spatial: &spatial,
    });
    assert!(
        result.is_ok(),
        "tavern feature validation should pass, issues: {:?}",
        result.issues
    );
}

#[test]
fn crypt_has_sarcophagus_in_vault() {
    let (_, _, _, features) = run_crypt_features();
    let has_sarcophagus = features
        .features
        .iter()
        .any(|f| f.kind == FeatureKind::Sarcophagus);
    assert!(
        has_sarcophagus,
        "crypt should have a sarcophagus in the family vault"
    );
}

#[test]
fn tavern_has_table_in_hub() {
    let (_, _, _, features) = run_tavern_features();
    let has_table = features
        .features
        .iter()
        .any(|f| f.kind == FeatureKind::Table);
    assert!(has_table, "tavern should have a table in the taproom hub");
}

#[test]
fn crypt_features_no_overlap() {
    let (_, _, _, features) = run_crypt_features();
    let mut all_cells = std::collections::HashSet::new();
    for f in &features.features {
        for &cell in &f.cells {
            assert!(
                all_cells.insert(cell),
                "crypt: feature cell ({}, {}) is used by multiple features",
                cell.x,
                cell.y
            );
        }
    }
}

#[test]
fn tavern_features_no_overlap() {
    let (_, _, _, features) = run_tavern_features();
    let mut all_cells = std::collections::HashSet::new();
    for f in &features.features {
        for &cell in &f.cells {
            assert!(
                all_cells.insert(cell),
                "tavern: feature cell ({}, {}) is used by multiple features",
                cell.x,
                cell.y
            );
        }
    }
}

#[test]
fn crypt_ascii_output_contains_feature_chars() {
    let (_, _, map, features) = run_crypt_features();
    let output = procgen::tile::ascii::render_ascii_with_features(&map, &features);
    // Should contain at least one feature character.
    let feature_chars = ['\u{2020}', 'S', '$', 'o', '=', 'T', '^', '~'];
    let has_feature = feature_chars.iter().any(|&ch| output.contains(ch));
    assert!(
        has_feature,
        "crypt ASCII output should contain at least one feature char"
    );
}

#[test]
fn tavern_ascii_output_contains_feature_chars() {
    let (_, _, map, features) = run_tavern_features();
    let output = procgen::tile::ascii::render_ascii_with_features(&map, &features);
    let feature_chars = ['\u{2020}', 'S', '$', 'o', '=', 'T', '^', '~'];
    let has_feature = feature_chars.iter().any(|&ch| output.contains(ch));
    assert!(
        has_feature,
        "tavern ASCII output should contain at least one feature char"
    );
}

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

// ============================================================
// Phase 6: Entity placement integration tests
// ============================================================

use procgen::entity::plan::EntityArchetypeId;
use procgen::entity::planner::{EntityPlanner, SimpleEntityPlanner};
use procgen::validate::entity::{EntityValidationInput, EntityValidator};

/// Helper: run the crypt pipeline through entity placement.
fn run_crypt_entities() -> (
    procgen::spatial::plan::SpatialPlan,
    procgen::geometry::geom::GeometryPlan,
    procgen::tile::map::TileMap,
    procgen::feature::plan::FeaturePlan,
    procgen::entity::plan::EntityPlan,
) {
    let (spatial, geometry, map, features) = run_crypt_features();
    let mut rng = StdRng::seed_from_u64(42);
    let entities = SimpleEntityPlanner
        .plan(&spatial, &geometry, &map, &features, &mut rng)
        .unwrap();
    (spatial, geometry, map, features, entities)
}

/// Helper: run the tavern pipeline through entity placement.
fn run_tavern_entities() -> (
    procgen::spatial::plan::SpatialPlan,
    procgen::geometry::geom::GeometryPlan,
    procgen::tile::map::TileMap,
    procgen::feature::plan::FeaturePlan,
    procgen::entity::plan::EntityPlan,
) {
    let (spatial, geometry, map, features) = run_tavern_features();
    let mut rng = StdRng::seed_from_u64(42);
    let entities = SimpleEntityPlanner
        .plan(&spatial, &geometry, &map, &features, &mut rng)
        .unwrap();
    (spatial, geometry, map, features, entities)
}

#[test]
fn crypt_entity_plan_succeeds() {
    let (_, _, _, _, entities) = run_crypt_entities();
    assert!(
        !entities.entities.is_empty(),
        "crypt should have at least one entity placed"
    );
}

#[test]
fn tavern_entity_plan_succeeds() {
    let (_, _, _, _, entities) = run_tavern_entities();
    assert!(
        !entities.entities.is_empty(),
        "tavern should have at least one entity placed"
    );
}

#[test]
fn crypt_entities_pass_validation() {
    let (spatial, geometry, map, features, entities) = run_crypt_entities();
    let result = EntityValidator.validate(&EntityValidationInput {
        entities: &entities,
        features: &features,
        tiles: &map,
        geometry: &geometry,
        spatial: &spatial,
    });
    assert!(
        result.is_ok(),
        "crypt entity validation should pass, issues: {:?}",
        result.issues
    );
}

#[test]
fn tavern_entities_pass_validation() {
    let (spatial, geometry, map, features, entities) = run_tavern_entities();
    let result = EntityValidator.validate(&EntityValidationInput {
        entities: &entities,
        features: &features,
        tiles: &map,
        geometry: &geometry,
        spatial: &spatial,
    });
    assert!(
        result.is_ok(),
        "tavern entity validation should pass, issues: {:?}",
        result.issues
    );
}

#[test]
fn crypt_entities_no_feature_overlap() {
    let (_, _, _, features, entities) = run_crypt_entities();
    let feature_cells: std::collections::HashSet<Point> = features
        .features
        .iter()
        .flat_map(|f| f.cells.iter().copied())
        .collect();
    for entity in &entities.entities {
        assert!(
            !feature_cells.contains(&entity.position),
            "crypt: entity '{}' at ({}, {}) overlaps a feature",
            entity.archetype,
            entity.position.x,
            entity.position.y
        );
    }
}

#[test]
fn tavern_entities_no_feature_overlap() {
    let (_, _, _, features, entities) = run_tavern_entities();
    let feature_cells: std::collections::HashSet<Point> = features
        .features
        .iter()
        .flat_map(|f| f.cells.iter().copied())
        .collect();
    for entity in &entities.entities {
        assert!(
            !feature_cells.contains(&entity.position),
            "tavern: entity '{}' at ({}, {}) overlaps a feature",
            entity.archetype,
            entity.position.x,
            entity.position.y
        );
    }
}

#[test]
fn crypt_entry_has_no_entities() {
    let (spatial, _, _, _, entities) = run_crypt_entities();
    let entry_id = spatial
        .spaces
        .iter()
        .find(|s| s.role == procgen::intent::graph::NodeRole::Entry)
        .expect("crypt should have an entry space")
        .id;
    let entry_entities = entities.entities_in_space(entry_id);
    assert!(
        entry_entities.is_empty(),
        "entry room should have no entities, got: {:?}",
        entry_entities
            .iter()
            .map(|e| &e.archetype)
            .collect::<Vec<_>>()
    );
}

#[test]
fn tavern_entry_has_no_entities() {
    let (spatial, _, _, _, entities) = run_tavern_entities();
    let entry_id = spatial
        .spaces
        .iter()
        .find(|s| s.role == procgen::intent::graph::NodeRole::Entry)
        .expect("tavern should have an entry space")
        .id;
    let entry_entities = entities.entities_in_space(entry_id);
    assert!(
        entry_entities.is_empty(),
        "entry room should have no entities, got: {:?}",
        entry_entities
            .iter()
            .map(|e| &e.archetype)
            .collect::<Vec<_>>()
    );
}

#[test]
fn crypt_ascii_output_contains_entity_chars() {
    let (_, _, map, features, entities) = run_crypt_entities();
    let output = procgen::tile::ascii::render_ascii_with_entities(&map, &features, &entities);
    let entity_chars = ['r', 's', 'G', '@', 'M', 'E'];
    let has_entity = entity_chars.iter().any(|&ch| output.contains(ch));
    assert!(
        has_entity,
        "crypt ASCII output should contain at least one entity char"
    );
}

#[test]
fn tavern_ascii_output_contains_entity_chars() {
    let (_, _, map, features, entities) = run_tavern_entities();
    let output = procgen::tile::ascii::render_ascii_with_entities(&map, &features, &entities);
    let entity_chars = ['r', 's', 'G', '@', 'M', 'E'];
    let has_entity = entity_chars.iter().any(|&ch| output.contains(ch));
    assert!(
        has_entity,
        "tavern ASCII output should contain at least one entity char"
    );
}

#[test]
fn crypt_has_guardians_in_goal() {
    let (_, _, _, _, entities) = run_crypt_entities();
    let has_guardian = entities
        .entities
        .iter()
        .any(|e| e.archetype == EntityArchetypeId::from("skeleton_guardian"));
    assert!(
        has_guardian,
        "crypt should have skeleton guardians in the goal room"
    );
}

#[test]
fn tavern_has_smuggler() {
    let (_, _, _, _, entities) = run_tavern_entities();
    let has_smuggler = entities
        .entities
        .iter()
        .any(|e| e.archetype == EntityArchetypeId::from("smuggler"));
    assert!(
        has_smuggler,
        "tavern should have a smuggler in the tunnel room"
    );
}

// ============================================================
// Phase 7: Pipeline runner integration tests
// ============================================================

use procgen::pipeline::{Pipeline, PipelineConfig};

/// Run the crypt scenario through the Pipeline runner N times with different seeds.
/// All runs must succeed and produce valid output.
#[test]
fn pipeline_crypt_multiple_seeds_all_valid() {
    let situation = build_crypt_situation();
    for seed in 0..10u64 {
        let config = PipelineConfig {
            seed: Some(seed),
            ..PipelineConfig::default()
        };
        let pipeline = Pipeline { config };
        let result = pipeline.run(&situation, &CryptIntentBuilder);
        assert!(
            result.is_ok(),
            "crypt pipeline failed with seed {seed}: {:?}",
            result.err()
        );
        let r = result.unwrap();
        // Basic sanity: map is non-empty, has entities and features.
        assert!(r.tiles.width > 0);
        assert!(r.tiles.height > 0);
        assert!(!r.features.features.is_empty(), "seed {seed}: no features");
        assert!(!r.entities.entities.is_empty(), "seed {seed}: no entities");
    }
}

/// Run the tavern scenario through the Pipeline runner N times with different seeds.
#[test]
fn pipeline_tavern_multiple_seeds_all_valid() {
    let situation = build_tavern_situation();
    for seed in 100..110u64 {
        let config = PipelineConfig {
            seed: Some(seed),
            ..PipelineConfig::default()
        };
        let pipeline = Pipeline { config };
        let result = pipeline.run(&situation, &TavernIntentBuilder);
        assert!(
            result.is_ok(),
            "tavern pipeline failed with seed {seed}: {:?}",
            result.err()
        );
        let r = result.unwrap();
        assert!(r.tiles.width > 0);
        assert!(r.tiles.height > 0);
        assert!(!r.features.features.is_empty(), "seed {seed}: no features");
        assert!(!r.entities.entities.is_empty(), "seed {seed}: no entities");
    }
}

/// Same seed produces identical output (determinism test).
#[test]
fn pipeline_determinism_same_seed_same_output() {
    let situation = build_crypt_situation();
    let config = PipelineConfig {
        seed: Some(42),
        ..PipelineConfig::default()
    };

    let pipeline = Pipeline {
        config: config.clone(),
    };
    let r1 = pipeline.run(&situation, &CryptIntentBuilder).unwrap();

    let pipeline = Pipeline { config };
    let r2 = pipeline.run(&situation, &CryptIntentBuilder).unwrap();

    // Compare ASCII output as a proxy for full equality.
    let ascii1 =
        procgen::tile::ascii::render_ascii_with_entities(&r1.tiles, &r1.features, &r1.entities);
    let ascii2 =
        procgen::tile::ascii::render_ascii_with_entities(&r2.tiles, &r2.features, &r2.entities);
    assert_eq!(ascii1, ascii2, "same seed should produce identical output");
}

/// Pipeline with no seed (entropy) should still produce valid output.
#[test]
fn pipeline_no_seed_still_valid() {
    let situation = build_crypt_situation();
    let config = PipelineConfig {
        seed: None,
        ..PipelineConfig::default()
    };
    let pipeline = Pipeline { config };
    let result = pipeline.run(&situation, &CryptIntentBuilder);
    assert!(
        result.is_ok(),
        "pipeline without seed failed: {:?}",
        result.err()
    );
}
