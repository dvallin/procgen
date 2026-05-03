# AGENTS.md — Procgen Project Guide

## What This Is

A procedural dungeon/map generator written in Rust. It transforms high-level narrative context ("noble crypt, locked vault, optional chapel") into concrete tile maps with rooms, corridors, doors, features, and entities.

## Current State

Working vertical slice with full pipeline, property-based tests, and integration tests. Three demo scenarios (crypt, tavern, cave) produce ASCII output. All intent generation is data-driven — no hard-coded builders remain.

**Completed phases (original build-out):** Module restructure, Tag newtype, MapIntent + SituationContext, SpatialPlan + generic geometry, real geometry placement (footprints, routing, corridor merging, constraint-aware layout), FeaturePlan, EntityPlan, Pipeline Runner + Validation Loop (retry, relaxation, seeded RNG).

**Completed phases (new roadmap):**
- **Phase 1 — Data-Driven Rules & Asset Loading** (tasks 1.1–1.5): Feature/entity rules as JSON assets with embedded fallback, generic asset loader, PipelineConfig overrides.
- **Phase 2 — Narrative Patterns & Intent Generation** (tasks 2.1–2.8): NarrativePattern data model, 5 starter patterns, pattern selector (weighted voting + RNG), 2 theme vocabularies, slot filler, structural constraint inference, GenericIntentBuilder, regression tests. Hard-coded fixture builders removed; `Pipeline::run()` uses GenericIntentBuilder internally.
- **Phase 3 — Tile Registry & Data-Driven Rasterization** (tasks 3.1–3.8): `TileId(u16)` newtype, `TileRegistry` with properties (walkable, opaque, ascii_char, tags), `TileMap` stores `Vec<TileId>`, backward-compat `Tile` namespace struct with named constants (`Tile::FLOOR`, etc.), JSON asset for tile definitions. Rasterizer accepts `&TileRegistry`. Extended palette (Water, Pit, Stairs, Rubble, Grass). ASCII renderer uses registry for tile characters (`render_ascii_full`). **Tile scatter rules** (`assets/rules/tile_scatter.json`) replace floor tiles with terrain variants at data-driven densities during rasterization — proper separation: tiles ARE the ground (water, rubble, grass), features are objects ON tiles (moss, stalagmites, fungus). Non-walkable scatter is connectivity-safe.
- **Phase 4.1 — Feature Registry & FeatureCategory enum**: `FeatureType(String)` newtype replaces `FeatureKind` enum. `FeatureRegistry` maps type name → `FeatureProperties { category, ascii_char, blocking, tags }`. `FeatureCategory` enum (Furniture, Container, Trap, Decoration, Interactable) stays for behavior dispatch. Feature rules reference type strings ("altar", "chest") instead of enum variants. New feature types added via `assets/rules/feature_types.json` without code changes.
- **Phase 4.2 — Tag classification & atmosphere profiles**: `SpaceSpec` gains `structural_tags`, `atmosphere_tags`, `motifs` (replacing flat `tags`). `AtmosphereProfile` bundles weighted scatter/feature/entity influences keyed by tag match. `AtmospherePalette` merges active profiles; the planner samples from the palette. Profiles loaded from `assets/rules/atmospheres.json`.
- **Phase 4.3 — Room zones & InteriorPlan**: `InteriorPlan` partitions each room into zones (`Center`, `WallBand`, `Corner`, `DoorPath`, `Open`). Reserved door-to-door paths computed once via BFS — scatter and feature planner both exclude reserved cells for non-walkable/blocking placements. `doors_reachable` BFS retained as safety-net. Pipeline: rasterize → reserve paths → scatter → build interiors → feature plan.
- **Phase 4.4 — Template interiors (constraint-based)**: `InteriorTemplate` declarative data model: match rooms by role/archetype/tag, assign zone→feature directives (`Place { feature_type, max_count }` or `Clear`). Templates loaded from `assets/rules/interior_templates.json` (6 starters: crypt_vault, sacred_chamber, storage_room, treasure_room, hub_hall, guard_post). Feature planner uses zone cells from `InteriorPlan` for template-matched rooms; `Clear` directives prevent blocking features in those zones. Atmosphere rules still apply after template placement (respecting clear zones). Templates configurable via `PipelineConfig.interior_templates`.
- **Phase 4.5 — Irregular room shapes**: `ShapeRefinement` trait separates layout (WHERE rooms go) from shape (WHAT each room looks like). `RectShape` (no-op default) + `CaveIrregularizer` (carve corners, roughen edges, protect center cross, connectivity BFS). Rooms with `LocationKind::Cave` or atmosphere tag `"natural"` on rooms ≥ 7×7 get irregular footprints. `GeometryPlanner::plan` now takes `&mut dyn RngCore` for deterministic shape generation. Rasterizer gains `connect_doors_to_rooms` step: carves floor from each door inward until reaching existing floor, ensuring connectivity for irregular rooms. `SpatialPlan` carries `location_kind` from `MapIntent`. Geometry validator uses `space.rect` (not `footprint.bounding_rect()`) for boundary checks. Natural cave vocabulary now uses `LocationKind::Cave`.

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
│   └── cave.rs          # build_cave_situation() → SituationContext
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
cargo run -- --seed 42             # Deterministic generation with seed
cargo run -- crypt --seed 42       # Explicit scenario + seed
cargo run -- --trace               # INFO-level pipeline trace (to stderr)
cargo run -- --trace debug         # DEBUG-level (placement details, routing decisions)
cargo run -- --trace all           # TRACE-level (everything)
cargo run -- --help                # Show CLI usage
RUST_LOG=procgen=debug cargo run -- --trace  # Override via env var
cargo test       # Runs all tests (349 currently: 268 unit/proptest + 23 regression + 54 integration + 4 doctests)
```

## Target Architecture

See `PLAN.md` for the full evolution plan from current state to the target layered architecture.
