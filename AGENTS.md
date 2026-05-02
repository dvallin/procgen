# AGENTS.md — Procgen Project Guide

## What This Is

A procedural dungeon/map generator written in Rust. It transforms high-level narrative context ("noble crypt, locked vault, optional chapel") into concrete tile maps with rooms, corridors, doors, features, and entities.

## Current State

Working vertical slice with full pipeline, property-based tests, and integration tests. Two demo scenarios (crypt, tavern) produce ASCII output. All intent generation is data-driven — no hard-coded builders remain.

**Completed phases (original build-out):** Module restructure, Tag newtype, MapIntent + SituationContext, SpatialPlan + generic geometry, real geometry placement (footprints, routing, corridor merging, constraint-aware layout), FeaturePlan, EntityPlan, Pipeline Runner + Validation Loop (retry, relaxation, seeded RNG).

**Completed phases (new roadmap):**
- **Phase 1 — Data-Driven Rules & Asset Loading** (tasks 1.1–1.5): Feature/entity rules as JSON assets with embedded fallback, generic asset loader, PipelineConfig overrides.
- **Phase 2 — Narrative Patterns & Intent Generation** (tasks 2.1–2.8): NarrativePattern data model, 5 starter patterns, pattern selector (weighted voting + RNG), 2 theme vocabularies, slot filler, structural constraint inference, GenericIntentBuilder, regression tests. Hard-coded fixture builders removed; `Pipeline::run()` uses GenericIntentBuilder internally.

## Pipeline (current)

```
SituationContext → IntentBuilder → MapIntent → SpatialPlan → GeometryPlan → TileMap → FeaturePlan → EntityPlan → ASCII
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
│   └── tavern.rs        # build_tavern_situation() → SituationContext
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
│   ├── planner.rs       # SpatialPlan → GeometryPlan (BFS layout + constraint-aware placement)
│   └── routing.rs       # CorridorRouter trait + ZShapeRouter (face-based routing with corridor merging)
├── tile/                # Tile-level rasterization
│   ├── map.rs           # TileMap, Tile enum, flood_fill
│   ├── rasterize.rs     # GeometryPlan → TileMap (trait + impl + proptest)
│   └── ascii.rs         # TileMap → String debug rendering (+ feature + entity overlay)
├── feature/             # Feature placement
│   ├── plan.rs          # FeaturePlan, FeaturePlacement, FeatureKind, FeaturePlanError
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
    └── entities.json    # Default entity rules (serde JSON, embedded fallback)

tests/
├── integration.rs       # End-to-end pipeline tests (connectivity, overlaps, etc.)
└── generic_builder_regression.rs  # Regression tests: GenericIntentBuilder vs fixture builders
```

## Key Utility Methods

These live on the types they belong to — no separate utility module:

- **`Point::cardinals()`** — returns `[Point; 4]` (N, S, W, E neighbors)
- **`Point::neighbors()`** — returns `[Point; 8]` (cardinal + diagonal)
- **`Rect::overlaps(&self, other: &Rect)`** — exclusive overlap test
- **`Tile::is_walkable()`** — true for Floor, Door, LockedDoor
- **`Tile::is_solid()`** — true for Void, Wall
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
cargo run -- --seed 42             # Deterministic generation with seed
cargo run -- crypt --seed 42       # Explicit scenario + seed
cargo run -- --trace               # INFO-level pipeline trace (to stderr)
cargo run -- --trace debug         # DEBUG-level (placement details, routing decisions)
cargo run -- --trace all           # TRACE-level (everything)
cargo run -- --help                # Show CLI usage
RUST_LOG=procgen=debug cargo run -- --trace  # Override via env var
cargo test       # Runs all tests (289 currently: 210 unit/proptest + 23 regression + 54 integration + 3 doctests)
```

## Target Architecture

See `PLAN.md` for the full evolution plan from current state to the target layered architecture.
