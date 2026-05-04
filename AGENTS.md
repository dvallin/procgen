# AGENTS.md — Procgen Project Guide

## What This Is

A procedural dungeon/map generator written in Rust. It transforms high-level narrative context ("noble crypt, locked vault, optional chapel") into concrete tile maps with rooms, corridors, doors, features, and entities.

## Current State

Working vertical slice with full pipeline, property-based tests, and integration tests. Four demo scenarios (crypt, tavern, cave, cellar) produce ASCII output. All intent generation is data-driven — no hard-coded builders remain.

**Completed phases (original build-out):** Module restructure, Tag newtype, MapIntent + SituationContext, SpatialPlan + generic geometry, real geometry placement (footprints, routing, corridor merging, constraint-aware layout), FeaturePlan, EntityPlan, Pipeline Runner + Validation Loop (retry, relaxation, seeded RNG).

**Completed phases (new roadmap):**
- **Phase 1 — Data-Driven Rules & Asset Loading** (tasks 1.1–1.5): Feature/entity rules as JSON assets with embedded fallback, generic asset loader, PipelineConfig overrides.
- **Phase 2 — Narrative Patterns & Intent Generation** (tasks 2.1–2.8): NarrativePattern data model, 5 starter patterns, pattern selector (weighted voting + RNG), 2 theme vocabularies, slot filler, structural constraint inference, GenericIntentBuilder, regression tests. Hard-coded fixture builders removed; `Pipeline::run()` uses GenericIntentBuilder internally.
- **Phase 3 — Tile Registry & Data-Driven Rasterization** (tasks 3.1–3.8): `TileId(u16)` newtype, `TileRegistry` with properties, `TileMap` stores `Vec<TileId>`, JSON asset for tile definitions, extended palette (Water, Pit, Stairs, Rubble, Grass), tile scatter rules (connectivity-safe).
- **Phase 4 — Room Character, Atmosphere & World-Engine Flexibility** (tasks 4.1–4.12): Feature registry (`FeatureType(String)` + `FeatureRegistry`), tag classification (structural/atmosphere/motifs), atmosphere profiles (weighted palettes per room), room zones + InteriorPlan, interior templates (6 starters), irregular room shapes (CaveIrregularizer), vermin cellar scenario, explicit pattern override, vocabulary overlays (base + override), additive rule injection, PipelineResult annotations (room letters + legend), situation directives (`PinEntity` for named quest entities). Proves the full design thesis: a world engine can control structure, compose themes, inject rules, pin entities, and read back annotated results — all without per-scenario code.
- **Phase 5.1 — Force-Directed Geometry Planner**: `ForceDirectedGeometryPlanner` uses physics simulation (attraction along edges, repulsion between non-adjacent, overlap resolution, constraint enforcement) to produce natural spatial layouts for larger room counts (10–15+). `SimpleGeometryPlanner` renamed to `ColumnGeometryPlanner`. Both available behind `GeometryPlanner` trait; `PipelineConfig.geometry_strategy` selects which to use (`GeometryStrategy::Column` or `GeometryStrategy::ForceDirected`). CLI gains `--layout` flag (`column`/`force`). Default: force-directed.

## Pipeline (current)

```
SituationContext → IntentBuilder → MapIntent → SpatialPlan → GeometryPlan → TileMap → reserve paths → scatter → InteriorPlan → FeaturePlan → EntityPlan → ASCII
```

Each stage is behind a trait (`IntentBuilder`, `SpatialPlanner`, `GeometryPlanner`, `Rasterizer`, `FeaturePlanner`, `EntityPlanner`) with a simple default implementation. The `Pipeline` struct orchestrates all stages with retry semantics and seeded RNG.

## Module Layout

```
src/
├── main.rs              # CLI entry point, thin wrapper around Pipeline::run
├── lib.rs               # Crate root, re-exports modules
├── tag.rs               # Tag newtype (wraps String)
├── pipeline.rs          # Pipeline runner, PipelineConfig, PipelineError, retry loop + seeded RNG
├── demo/                # Scenario situation factories
│   ├── crypt.rs         # build_crypt_situation() → SituationContext
│   ├── tavern.rs        # build_tavern_situation() → SituationContext
│   ├── cave.rs          # build_cave_situation() → SituationContext
│   └── cellar.rs        # build_cellar_situation() → SituationContext
├── situation/           # World/narrative context
│   └── mod.rs           # SituationContext struct + builder methods
├── intent/              # Map intent / structural graph
│   ├── graph.rs         # ScenarioGraph, ScenarioNode, ScenarioEdge
│   ├── map_intent.rs    # MapIntent, LocationKind, MapScale, IntentConstraint
│   ├── builder.rs       # IntentBuilder trait, IntentBuildError
│   ├── pattern.rs       # NarrativePattern, PatternSlot, PatternEdge, PatternVote
│   ├── selector.rs      # score_pattern, rank_patterns, select_pattern (weighted voting + RNG tiebreak)
│   ├── vocabulary.rs    # ThemeVocabulary, RoleVocabulary, VocabularyEntry
│   ├── filler.rs        # fill_pattern: slot instantiation with vocabulary + RNG
│   ├── constraints.rs   # infer_constraints: structural MustGate + MaxDepth from filled graph
│   ├── generic_builder.rs # GenericIntentBuilder: data-driven IntentBuilder impl
│   ├── template.rs      # Serde-friendly template asset types
│   └── instantiate.rs   # Template → ScenarioGraph conversion
├── spatial/             # Abstract spatial planning
│   ├── plan.rs          # SpatialPlan, SpaceSpec, SpaceLink, SpaceArchetype, SizeHint, SpatialConstraint
│   └── planner.rs       # MapIntent → SpatialPlan (trait + impl + resolve_dimensions + proptest)
├── geometry/            # Concrete geometry placement
│   ├── geom.rs          # Point, Rect, RectPointIter, Footprint, PlacedSpace, GeometryPlan
│   ├── common.rs        # Shared helpers (get_space_dimensions, build_adjacency, find_bfs_root, normalize_positions, apply_shape_refinement)
│   ├── planner.rs       # GeometryPlanner trait + ColumnGeometryPlanner (BFS column layout)
│   ├── force_directed.rs # ForceDirectedGeometryPlanner (physics simulation layout)
│   ├── routing.rs       # CorridorRouter trait + ZShapeRouter (face-based routing with corridor merging)
│   └── shape.rs         # ShapeRefinement trait + RectShape (no-op) + CaveIrregularizer (organic cave shapes)
├── tile/                # Tile-level rasterization
│   ├── registry.rs      # TileId newtype, Tile constants, TileProperties, TileRegistry
│   ├── map.rs           # TileMap (stores Vec<TileId>), flood_fill
│   ├── rasterize.rs     # GeometryPlan → TileMap (trait + impl + proptest)
│   ├── scatter.rs       # TileScatterRule, apply_scatter (tag→tile replacement with density, reserved-path aware)
│   └── ascii.rs         # TileMap → String debug rendering (+ feature + entity overlay)
├── interior/            # Room interior planning (zones + reserved paths)
│   ├── plan.rs          # InteriorPlan, Zone, ZoneKind, PathIntent
│   ├── paths.rs         # find_room_doors, compute_reserved_paths (per-room BFS)
│   ├── zones.rs         # classify_zones (Center, WallBand, Corner, DoorPath, Open)
│   ├── builder.rs       # build_interior_plans: GeometryPlan + TileMap → Vec<InteriorPlan>
│   └── template.rs      # InteriorTemplate, ZoneDirective, ZoneAction, match_template
├── feature/             # Feature placement
│   ├── registry.rs      # FeatureType newtype, FeatureCategory enum, FeatureProperties, FeatureRegistry
│   ├── plan.rs          # FeaturePlan, FeaturePlacement, Feature constants, FeaturePlanError
│   ├── placement.rs     # PlacementStrategy enum + tile-finding helpers
│   ├── rules.rs         # FeatureRule, default_rules(), matching_rules()
│   └── planner.rs       # FeaturePlanner trait + SimpleFeaturePlanner impl
├── entity/              # Entity placement
│   ├── plan.rs          # EntityPlan, EntityPlacement, EntityArchetypeId, EntityPlanError
│   ├── rules.rs         # EntityRule, EntityPlacementStrategy, default_entity_rules(), matching_entity_rules()
│   └── planner.rs       # EntityPlanner trait + SimpleEntityPlanner impl
├── validate/            # Validation traits
│   ├── mod.rs           # Validator<T> trait, Severity, ValidationResult
│   ├── geometry.rs      # GeometryValidator
│   ├── feature.rs       # FeatureValidator
│   └── entity.rs        # EntityValidator
└── asset/               # Asset loading utilities
    └── load.rs

assets/
├── patterns/
│   └── narrative.json   # Narrative patterns (5 starter patterns, embedded fallback)
├── vocabularies/
│   └── themes.json      # Theme vocabularies (undead_nobility, urban_underground)
└── rules/
    ├── features.json    # Default feature rules (serde JSON, embedded fallback)
    ├── feature_types.json # Feature type registry (name→category+char+blocking+tags)
    ├── entities.json    # Default entity rules (serde JSON, embedded fallback)
    ├── tiles.json       # Default tile registry (serde JSON, embedded fallback)
    ├── tile_scatter.json # Tile scatter rules (tag→tile density, embedded fallback)
    └── interior_templates.json # Interior templates (zone→feature directives, embedded fallback)

tests/
├── integration.rs       # End-to-end pipeline tests (connectivity, overlaps, etc.)
└── generic_builder_regression.rs  # Regression tests: GenericIntentBuilder vs fixture builders
```

## Key Utility Methods

These live on the types they belong to — no separate utility module:

- **`Point::cardinals()`** — returns `[Point; 4]` (N, S, W, E neighbors)
- **`Point::neighbors()`** — returns `[Point; 8]` (cardinal + diagonal)
- **`Rect::overlaps(&self, other: &Rect)`** — exclusive overlap test
- **`Rect::gap_to(&self, other: &Rect)`** — minimum axis-aligned gap (0 if overlap/touch)
- **`Rect::gap_x(&self, other: &Rect)`** — x-axis gap (0 if overlap along x)
- **`Rect::gap_y(&self, other: &Rect)`** — y-axis gap (0 if overlap along y)
- **`TileId::is_walkable()`** — true for Floor, Door, LockedDoor (built-in tiles)
- **`TileId::is_solid()`** — true for Void, Wall (built-in tiles)
- **`TileRegistry::is_walkable(id)`** — registry-based check (works for custom tiles too)
- **`TileMap::flood_fill(start, passable)`** — BFS returning `HashSet<Point>`

## Architecture Principles

1. **Data flows down through typed layers.** Each layer's output struct is the contract for the next layer's input.
2. **Traits at boundaries.** Every transform is a trait (`SpatialPlanner`, `GeometryPlanner`, `Rasterizer`) so implementations can be swapped.
3. **Tags for soft intent.** Layers communicate optional/contextual information via `Vec<Tag>` (a newtype around `String`).
4. **Keep the demo green.** Every refactor must preserve a working `cargo run` that produces valid output.
5. **Top-down, then depth.** Build the full pipeline skeleton with simple implementations before investing in algorithmic sophistication.

## Conventions

- **Error types:** One enum per module boundary (e.g. `GeometryPlanError`, `SpatialPlanError`). Implement `Display` and `Error`.
- **Naming:** Traits are verbs/agent nouns (`SpatialPlanner`, `Rasterizer`). Structs are nouns describing data (`GeometryPlan`, `TileMap`).
- **No panics in library code.** Return `Result` from all trait methods. Demos may `unwrap`.
- **Hardcoded values are temporary.** Mark them with comments or isolate them in functions that will later become configurable.
- **Tests:** Property-based tests (proptest) for layer invariants that hold for any input. Integration tests for the full pipeline with known demo scenarios.

## Running

```sh
cargo run                          # Prints ASCII map (default: crypt)
cargo run -- tavern                # Prints tavern cellar map
cargo run -- cave                  # Prints natural cave map
cargo run -- cellar                # Prints rat-infested port cellar map
cargo run -- --seed 42             # Deterministic generation with seed
cargo run -- crypt --seed 42       # Explicit scenario + seed
cargo run -- --annotate            # Room letters on map + legend below
cargo run -- --layout column       # Use BFS column layout (original)
cargo run -- --layout force        # Use force-directed layout (default)
cargo run -- --trace               # INFO-level pipeline trace (to stderr)
cargo run -- --trace debug         # DEBUG-level (placement details, routing decisions)
cargo run -- --trace all           # TRACE-level (everything)
cargo run -- --help                # Show CLI usage
RUST_LOG=procgen=debug cargo run -- --trace  # Override via env var
cargo test       # Runs all tests (401 currently: 304 unit/proptest + 23 regression + 70 integration + 4 doctests)
```

## Target Architecture

See `PLAN.md` for the full evolution plan from current state to the target layered architecture.
