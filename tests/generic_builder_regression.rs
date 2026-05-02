//! Regression tests for `GenericIntentBuilder` (task 2.8).
//!
//! Verifies that the data-driven `GenericIntentBuilder` produces structurally
//! valid output for both crypt and tavern scenarios, proving it can replace the
//! hard-coded `CryptIntentBuilder` and `TavernIntentBuilder` fixtures.

use procgen::demo::crypt::build_crypt_situation;
use procgen::demo::tavern::build_tavern_situation;
use procgen::intent::builder::IntentBuilder;
use procgen::intent::generic_builder::GenericIntentBuilder;
use procgen::intent::graph::{EdgeRole, NodeRole};
use procgen::intent::map_intent::{IntentConstraint, LocationKind, MapIntent};
use procgen::pipeline::{Pipeline, PipelineConfig};
use procgen::situation::SituationContext;
use procgen::tile::ascii::render_ascii_with_entities;
use rand::SeedableRng;
use rand::rngs::StdRng;

// ============================================================================
// Helpers
// ============================================================================

/// Build the crypt situation context (same as `build_crypt_situation` from demo).
fn crypt_situation() -> SituationContext {
    build_crypt_situation()
}

/// Build the tavern situation context (same as `build_tavern_situation` from demo).
fn tavern_situation() -> SituationContext {
    build_tavern_situation()
}

/// Construct the default `GenericIntentBuilder` loaded from embedded assets.
fn default_builder() -> GenericIntentBuilder {
    GenericIntentBuilder::from_defaults().expect("embedded assets should load successfully")
}

/// Build a `MapIntent` for the crypt scenario with the given seed.
fn build_crypt_intent(seed: u64) -> MapIntent {
    let builder = default_builder();
    let situation = crypt_situation();
    let mut rng = StdRng::seed_from_u64(seed);
    builder
        .build(&situation, &mut rng)
        .expect("crypt intent should build successfully")
}

/// Build a `MapIntent` for the tavern scenario with the given seed.
fn build_tavern_intent(seed: u64) -> MapIntent {
    let builder = default_builder();
    let situation = tavern_situation();
    let mut rng = StdRng::seed_from_u64(seed);
    builder
        .build(&situation, &mut rng)
        .expect("tavern intent should build successfully")
}

// ============================================================================
// Crypt structural tests
// ============================================================================

/// The crypt situation (sealed_crypt +2, locked_vault +3) should select the
/// `lock_and_key` pattern, which maps to `LocationKind::Dungeon` via the
/// `undead_nobility` vocabulary.
#[test]
fn crypt_location_kind_is_dungeon() {
    for seed in 0..20u64 {
        let intent = build_crypt_intent(seed);
        assert_eq!(
            intent.location_kind,
            LocationKind::Dungeon,
            "seed {seed}: crypt should always be Dungeon"
        );
    }
}

/// The lock_and_key pattern has 4 required slots: Entry, Hub, Gate, Goal.
/// All must be present regardless of seed.
#[test]
fn crypt_always_has_required_roles() {
    for seed in 0..20u64 {
        let intent = build_crypt_intent(seed);
        let graph = &intent.structural_graph;

        let has_entry = graph.nodes.iter().any(|n| n.role == NodeRole::Entry);
        let has_hub = graph.nodes.iter().any(|n| n.role == NodeRole::Hub);
        let has_gate = graph.nodes.iter().any(|n| n.role == NodeRole::Gate);
        let has_goal = graph.nodes.iter().any(|n| n.role == NodeRole::Goal);

        assert!(has_entry, "seed {seed}: crypt must have Entry node");
        assert!(has_hub, "seed {seed}: crypt must have Hub node");
        assert!(has_gate, "seed {seed}: crypt must have Gate node");
        assert!(has_goal, "seed {seed}: crypt must have Goal node");
    }
}

/// The lock_and_key pattern has RestrictedTraversal from gate to goal.
#[test]
fn crypt_has_restricted_traversal() {
    for seed in 0..20u64 {
        let intent = build_crypt_intent(seed);
        let has_restricted = intent
            .structural_graph
            .edges
            .iter()
            .any(|e| e.role == EdgeRole::RestrictedTraversal);
        assert!(
            has_restricted,
            "seed {seed}: crypt should have RestrictedTraversal edge"
        );
    }
}

/// The crypt should always produce a MustGate constraint (inferred from
/// Gate + RestrictedTraversal).
#[test]
fn crypt_has_must_gate_constraint() {
    for seed in 0..20u64 {
        let intent = build_crypt_intent(seed);
        let has_must_gate = intent
            .constraints
            .iter()
            .any(|c| matches!(c, IntentConstraint::MustGate { .. }));
        assert!(
            has_must_gate,
            "seed {seed}: crypt intent should have MustGate constraint, got: {:?}",
            intent.constraints
        );
    }
}

/// The undead_nobility vocabulary has "gothic" as a motif.
#[test]
fn crypt_motifs_include_gothic() {
    for seed in 0..20u64 {
        let intent = build_crypt_intent(seed);
        let motif_strs: Vec<&str> = intent.motifs.iter().map(|m| m.0.as_str()).collect();
        assert!(
            motif_strs.contains(&"gothic"),
            "seed {seed}: crypt motifs should include 'gothic', got: {:?}",
            motif_strs
        );
    }
}

/// Vocabulary fills archetype_hint for every node.
#[test]
fn crypt_all_nodes_have_archetype_hint() {
    for seed in 0..20u64 {
        let intent = build_crypt_intent(seed);
        for node in &intent.structural_graph.nodes {
            assert!(
                node.archetype_hint.is_some(),
                "seed {seed}: node '{}' (role {:?}) should have archetype_hint",
                node.key,
                node.role
            );
        }
    }
}

/// The lock_and_key pattern has 4 required + 2 optional slots.
/// Node count should be between 4 and 6.
#[test]
fn crypt_node_count_in_range() {
    for seed in 0..20u64 {
        let intent = build_crypt_intent(seed);
        let count = intent.structural_graph.nodes.len();
        assert!(
            (4..=6).contains(&count),
            "seed {seed}: crypt should have 4-6 nodes, got {count}"
        );
    }
}

/// Every edge must reference valid node IDs.
#[test]
fn crypt_edges_reference_valid_nodes() {
    for seed in 0..20u64 {
        let intent = build_crypt_intent(seed);
        let graph = &intent.structural_graph;
        let node_ids: Vec<_> = graph.nodes.iter().map(|n| n.id).collect();

        for edge in &graph.edges {
            assert!(
                node_ids.contains(&edge.from),
                "seed {seed}: edge from {:?} references missing node",
                edge.from
            );
            assert!(
                node_ids.contains(&edge.to),
                "seed {seed}: edge to {:?} references missing node",
                edge.to
            );
        }
    }
}

// ============================================================================
// Tavern structural tests
// ============================================================================

/// The tavern situation (tavern +2, dock_district +1) should select the
/// `hub_and_spoke` pattern, which maps to `LocationKind::Building` via the
/// `urban_underground` vocabulary.
#[test]
fn tavern_location_kind_is_building() {
    for seed in 0..20u64 {
        let intent = build_tavern_intent(seed);
        assert_eq!(
            intent.location_kind,
            LocationKind::Building,
            "seed {seed}: tavern should always be Building"
        );
    }
}

/// The hub_and_spoke pattern has 3 required slots: Entry, Hub, Goal (spoke_1).
#[test]
fn tavern_always_has_required_roles() {
    for seed in 0..20u64 {
        let intent = build_tavern_intent(seed);
        let graph = &intent.structural_graph;

        let has_entry = graph.nodes.iter().any(|n| n.role == NodeRole::Entry);
        let has_hub = graph.nodes.iter().any(|n| n.role == NodeRole::Hub);
        let has_goal = graph.nodes.iter().any(|n| n.role == NodeRole::Goal);

        assert!(has_entry, "seed {seed}: tavern must have Entry node");
        assert!(has_hub, "seed {seed}: tavern must have Hub node");
        assert!(has_goal, "seed {seed}: tavern must have Goal node");
    }
}

/// The hub_and_spoke pattern has no Gate slot — this is a deliberate difference
/// from the hard-coded TavernIntentBuilder fixture.
#[test]
fn tavern_does_not_have_gate_role() {
    for seed in 0..20u64 {
        let intent = build_tavern_intent(seed);
        let has_gate = intent
            .structural_graph
            .nodes
            .iter()
            .any(|n| n.role == NodeRole::Gate);
        assert!(
            !has_gate,
            "seed {seed}: hub_and_spoke has no Gate slot, but found one"
        );
    }
}

/// When the `hidden` optional slot is included, there should be a
/// SecretTraversal edge from hub to hidden.
#[test]
fn tavern_has_secret_traversal_when_hidden_present() {
    for seed in 0..50u64 {
        let intent = build_tavern_intent(seed);
        let graph = &intent.structural_graph;

        let has_hidden = graph.nodes.iter().any(|n| n.key == "hidden");
        let has_secret = graph
            .edges
            .iter()
            .any(|e| e.role == EdgeRole::SecretTraversal);

        if has_hidden {
            assert!(
                has_secret,
                "seed {seed}: hidden node present but no SecretTraversal edge"
            );
        }
    }
}

/// The urban_underground vocabulary has "timber" as a motif.
#[test]
fn tavern_motifs_include_timber() {
    for seed in 0..20u64 {
        let intent = build_tavern_intent(seed);
        let motif_strs: Vec<&str> = intent.motifs.iter().map(|m| m.0.as_str()).collect();
        assert!(
            motif_strs.contains(&"timber"),
            "seed {seed}: tavern motifs should include 'timber', got: {:?}",
            motif_strs
        );
    }
}

/// Vocabulary fills archetype_hint for every node.
#[test]
fn tavern_all_nodes_have_archetype_hint() {
    for seed in 0..20u64 {
        let intent = build_tavern_intent(seed);
        for node in &intent.structural_graph.nodes {
            assert!(
                node.archetype_hint.is_some(),
                "seed {seed}: node '{}' (role {:?}) should have archetype_hint",
                node.key,
                node.role
            );
        }
    }
}

/// The hub_and_spoke pattern has 3 required + 3 optional slots.
/// Node count should be between 3 and 6.
#[test]
fn tavern_node_count_in_range() {
    for seed in 0..20u64 {
        let intent = build_tavern_intent(seed);
        let count = intent.structural_graph.nodes.len();
        assert!(
            (3..=6).contains(&count),
            "seed {seed}: tavern should have 3-6 nodes, got {count}"
        );
    }
}

/// Every edge must reference valid node IDs.
#[test]
fn tavern_edges_reference_valid_nodes() {
    for seed in 0..20u64 {
        let intent = build_tavern_intent(seed);
        let graph = &intent.structural_graph;
        let node_ids: Vec<_> = graph.nodes.iter().map(|n| n.id).collect();

        for edge in &graph.edges {
            assert!(
                node_ids.contains(&edge.from),
                "seed {seed}: edge from {:?} references missing node",
                edge.from
            );
            assert!(
                node_ids.contains(&edge.to),
                "seed {seed}: edge to {:?} references missing node",
                edge.to
            );
        }
    }
}

// ============================================================================
// Full pipeline end-to-end tests
// ============================================================================

/// `GenericIntentBuilder` + crypt situation runs through the full Pipeline
/// (with retry loop) successfully for seeds 0..20.
#[test]
fn pipeline_crypt_all_seeds_succeed() {
    let situation = crypt_situation();

    for seed in 0..20u64 {
        let config = PipelineConfig {
            seed: Some(seed),
            ..PipelineConfig::default()
        };
        let pipeline = Pipeline { config };
        let result = pipeline.run(&situation);
        assert!(
            result.is_ok(),
            "crypt pipeline failed with seed {seed}: {:?}",
            result.err()
        );

        let r = result.unwrap();
        assert!(r.tiles.width > 0, "seed {seed}: map width should be > 0");
        assert!(r.tiles.height > 0, "seed {seed}: map height should be > 0");
    }
}

/// `GenericIntentBuilder` + tavern situation runs through the full Pipeline
/// successfully for seeds 0..20.
#[test]
fn pipeline_tavern_all_seeds_succeed() {
    let situation = tavern_situation();

    for seed in 0..20u64 {
        let config = PipelineConfig {
            seed: Some(seed),
            ..PipelineConfig::default()
        };
        let pipeline = Pipeline { config };
        let result = pipeline.run(&situation);
        assert!(
            result.is_ok(),
            "tavern pipeline failed with seed {seed}: {:?}",
            result.err()
        );

        let r = result.unwrap();
        assert!(r.tiles.width > 0, "seed {seed}: map width should be > 0");
        assert!(r.tiles.height > 0, "seed {seed}: map height should be > 0");
    }
}

/// Pipeline results should have non-empty features and entities for both scenarios.
#[test]
fn pipeline_results_have_features_and_entities() {
    // Crypt
    for seed in 0..20u64 {
        let config = PipelineConfig {
            seed: Some(seed),
            ..PipelineConfig::default()
        };
        let pipeline = Pipeline { config };
        let r = pipeline
            .run(&crypt_situation())
            .expect("crypt pipeline should succeed");
        assert!(
            !r.features.features.is_empty(),
            "crypt seed {seed}: should have features"
        );
        assert!(
            !r.entities.entities.is_empty(),
            "crypt seed {seed}: should have entities"
        );
    }

    // Tavern
    for seed in 0..20u64 {
        let config = PipelineConfig {
            seed: Some(seed),
            ..PipelineConfig::default()
        };
        let pipeline = Pipeline { config };
        let r = pipeline
            .run(&tavern_situation())
            .expect("tavern pipeline should succeed");
        assert!(
            !r.features.features.is_empty(),
            "tavern seed {seed}: should have features"
        );
        assert!(
            !r.entities.entities.is_empty(),
            "tavern seed {seed}: should have entities"
        );
    }
}

/// Determinism: same seed must produce identical ASCII output for crypt.
#[test]
fn pipeline_crypt_determinism() {
    let situation = crypt_situation();

    for seed in [0u64, 7, 13, 42, 99] {
        let config = PipelineConfig {
            seed: Some(seed),
            ..PipelineConfig::default()
        };

        let pipeline1 = Pipeline {
            config: config.clone(),
        };
        let r1 = pipeline1.run(&situation).unwrap();
        let ascii1 = render_ascii_with_entities(&r1.tiles, &r1.features, &r1.entities);

        let pipeline2 = Pipeline { config };
        let r2 = pipeline2.run(&situation).unwrap();
        let ascii2 = render_ascii_with_entities(&r2.tiles, &r2.features, &r2.entities);

        assert_eq!(
            ascii1, ascii2,
            "crypt seed {seed}: same seed should produce identical ASCII output"
        );
    }
}

/// Determinism: same seed must produce identical ASCII output for tavern.
#[test]
fn pipeline_tavern_determinism() {
    let situation = tavern_situation();

    for seed in [0u64, 7, 13, 42, 99] {
        let config = PipelineConfig {
            seed: Some(seed),
            ..PipelineConfig::default()
        };

        let pipeline1 = Pipeline {
            config: config.clone(),
        };
        let r1 = pipeline1.run(&situation).unwrap();
        let ascii1 = render_ascii_with_entities(&r1.tiles, &r1.features, &r1.entities);

        let pipeline2 = Pipeline { config };
        let r2 = pipeline2.run(&situation).unwrap();
        let ascii2 = render_ascii_with_entities(&r2.tiles, &r2.features, &r2.entities);

        assert_eq!(
            ascii1, ascii2,
            "tavern seed {seed}: same seed should produce identical ASCII output"
        );
    }
}

/// Different seeds should produce at least some variety in output (not all identical).
#[test]
fn pipeline_different_seeds_produce_variety() {
    // Crypt variety
    let mut crypt_outputs = std::collections::HashSet::new();
    for seed in 0..10u64 {
        let config = PipelineConfig {
            seed: Some(seed),
            ..PipelineConfig::default()
        };
        let pipeline = Pipeline { config };
        let r = pipeline.run(&crypt_situation()).unwrap();
        let ascii = render_ascii_with_entities(&r.tiles, &r.features, &r.entities);
        crypt_outputs.insert(ascii);
    }
    assert!(
        crypt_outputs.len() > 1,
        "10 different seeds should produce at least 2 distinct crypt maps"
    );

    // Tavern variety
    let mut tavern_outputs = std::collections::HashSet::new();
    for seed in 0..10u64 {
        let config = PipelineConfig {
            seed: Some(seed),
            ..PipelineConfig::default()
        };
        let pipeline = Pipeline { config };
        let r = pipeline.run(&tavern_situation()).unwrap();
        let ascii = render_ascii_with_entities(&r.tiles, &r.features, &r.entities);
        tavern_outputs.insert(ascii);
    }
    assert!(
        tavern_outputs.len() > 1,
        "10 different seeds should produce at least 2 distinct tavern maps"
    );
}

/// The generic builder should produce structurally compatible output that the
/// spatial planner can consume — verify the intent→spatial step works for
/// a range of seeds.
#[test]
fn generic_builder_intent_is_spatially_plannable() {
    use procgen::spatial::planner::{SimpleSpatialPlanner, SpatialPlanner};

    let builder = default_builder();

    for seed in 0..20u64 {
        let mut rng = StdRng::seed_from_u64(seed);

        let crypt_intent = builder.build(&crypt_situation(), &mut rng).unwrap();
        let crypt_spatial = SimpleSpatialPlanner.plan(&crypt_intent);
        assert!(
            crypt_spatial.is_ok(),
            "crypt seed {seed}: spatial planning failed: {:?}",
            crypt_spatial.err()
        );

        let mut rng = StdRng::seed_from_u64(seed);
        let tavern_intent = builder.build(&tavern_situation(), &mut rng).unwrap();
        let tavern_spatial = SimpleSpatialPlanner.plan(&tavern_intent);
        assert!(
            tavern_spatial.is_ok(),
            "tavern seed {seed}: spatial planning failed: {:?}",
            tavern_spatial.err()
        );
    }
}
