# AGENTS.md — Procgen Project Guide

## What This Is

A procedural dungeon/map generator written in Rust. It transforms high-level narrative context ("noble crypt, locked vault, optional chapel") into concrete tile maps with rooms, corridors, doors, features, and entities.

## Current State

Working vertical slice with property-based and integration tests. Two demo scenarios (`src/demo/crypt.rs`, `src/demo/tavern.rs`) flow through the full pipeline and produce ASCII output. Phase 1 (module restructure + Tag newtype), Phase 2 (MapIntent + SituationContext), and Phase 3 (enrich SpatialPlan + generic geometry) are complete. Phase 4 (real geometry placement) is in progress — Footprint abstraction, routing extraction, GeometryValidator, and constraint-aware placement done.

## Pipeline (current)

```
SituationContext → MapIntent → SpatialPlan → GeometryPlan → TileMap → ASCII
```

Each stage is behind a trait (`IntentBuilder`, `SpatialPlanner`, `GeometryPlanner`, `Rasterizer`) with a simple default implementation.

## Module Layout

```
src/
├── main.rs              # CLI entry point, runs the full pipeline
├── lib.rs               # Crate root, re-exports modules
├── tag.rs               # Tag newtype (wraps String)
├── demo/                # Hardcoded demo scenarios
│   ├── crypt.rs         # Noble crypt: situation → intent → pipeline
│   └── tavern.rs        # Tavern cellar: different topology + archetypes
├── situation/           # World/narrative context
│   └── mod.rs           # SituationContext struct + builder methods
├── intent/              # Map intent / structural graph
│   ├── graph.rs         # ScenarioGraph, ScenarioNode, ScenarioEdge
│   ├── map_intent.rs    # MapIntent, LocationKind, MapScale, IntentConstraint
│   ├── builder.rs       # IntentBuilder trait, IntentBuildError
│   ├── template.rs      # Serde-friendly template asset types
│   └── instantiate.rs   # Template → ScenarioGraph conversion
├── spatial/             # Abstract spatial planning
│   ├── plan.rs          # SpatialPlan, SpaceSpec, SpaceLink, SpaceArchetype, SizeHint, SpatialConstraint
│   └── planner.rs       # MapIntent → SpatialPlan (trait + impl + resolve_dimensions + proptest)
├── geometry/            # Concrete geometry placement
│   ├── geom.rs          # Point, Rect, Footprint, PlacedSpace, GeometryPlan
│   ├── planner.rs       # SpatialPlan → GeometryPlan (BFS layout + constraint-aware placement)
│   └── routing.rs       # CorridorRouter trait + ZShapeRouter (Z-shape corridor routing)
├── tile/                # Tile-level rasterization
│   ├── map.rs           # TileMap, Tile enum, flood_fill
│   ├── rasterize.rs     # GeometryPlan → TileMap (trait + impl + proptest)
│   └── ascii.rs         # TileMap → String debug rendering
├── feature/             # Feature placement (stub)
├── entity/              # Entity placement (stub)
├── validate/            # Validation traits
│   └── mod.rs           # Validator<T> trait, Severity, ValidationResult
└── asset/               # Asset loading utilities
    └── load.rs

tests/
└── integration.rs       # End-to-end pipeline tests (connectivity, overlaps, etc.)
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
cargo run        # Prints ASCII map (default: crypt)
cargo run -- tavern  # Prints tavern cellar map
cargo test       # Runs all tests (85 currently: 72 unit/proptest + 13 integration)
```

## Target Architecture

See `PLAN.md` for the full evolution plan from current state to the target layered architecture.
