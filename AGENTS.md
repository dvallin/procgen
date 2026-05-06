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
- **Phase 5.2 — Pattern Composition**: `PatternSlot` gains `expandable: bool`; `NarrativePattern` gains `expansion_votes: Vec<ExpansionVote>` for role-affinity scoring. When the filler encounters an expandable slot (and budget permits), it runs `select_pattern` on a filtered pattern library — reusing the same situation-tag voting mechanism as top-level selection. Sub-pattern graph is grafted into the parent with key namespacing (`branch_a.hub`, `branch_a.gate`). Budget controlled via `PipelineConfig.expansion_budget` or `SituationContext.bindings["expansion_budget"]`. Default: 0 (backward-compatible). Cave demo uses budget=1, producing 8–12 room maps. New `src/intent/compose.rs` module. Expandable slots: `branching_exploration.branch_a/b`, `hub_and_spoke.spoke_1`, `lock_and_key.key_area`. Expansion votes on `linear_descent` (Branch+2), `lock_and_key` (Goal+1, Branch+1), `gauntlet` (Gate+2, Goal+1).
- **Phase 5.3 — Entity Registry**: `EntityCategory` enum (`Creature`, `NPC`, `Hazard`, `Boss`) + `EntityProperties` struct + `EntityRegistry` (HashMap-based, mirrors `FeatureRegistry`). 11 built-in entity types defined in `assets/rules/entity_types.json` with embedded fallback. Hardcoded `entity_char()` substring matching replaced by registry lookups. ASCII render functions accept `&EntityRegistry`. `PipelineConfig.entity_registry` for overrides. New entities (bat, cave_spider) get dedicated chars; `giant_rat` gets uppercase 'R' for boss visibility.
- **Phase 5.4 — Pacing Curve (Tension)**: `TensionLevel` enum (`Low`, `Medium`, `High`, `Climax`) assigned to each room based on BFS distance from Entry normalized against the critical path (shortest Entry→Goal). `RoomAnnotation.tension` computed automatically. Goal rooms always `Climax`, Entry/Reward always `Low`. Tension visible in `--annotate` legend (`[Low]`, `[Medium]`, etc.) and `--trace debug` output. New `src/tension.rs` module with `compute_tension()` + 7 unit tests.
- **Phase 5.5 — Tension-Aware Entity Density**: `TensionLevel` gains `Ord` + `density_multiplier()` (Low=0.5×, Medium=1.0×, High=1.5×, Climax=2.0×). `EntityRule.tension_min: Option<TensionLevel>` gates rules by minimum room tension. Entity planner computes tension per-room, scales density cap by multiplier, filters rules (including atmosphere per-room rules) by tension. 4 new structural entity rules (skeleton×2, bat, cave_spider) gated at Medium/High; quest-critical rules (gate_guardian, skeleton_guardian) bypass tension. Result: low-tension rooms are sparse/safe, high-tension rooms pack threats.
- **Phase 5.6 — ANSI Colored Rendering**: `src/tile/color.rs` module adds colored ASCII output. Entities colored by danger (Boss=bold-red, Hazard=magenta, elite=yellow, Creature=red, NPC=green). Features colored by importance (Trap=red, Interactable=bold-cyan, Container=yellow, Furniture=blue, Decoration=dark-gray). Tiles get subtle coloring (walls=dark-gray, doors=yellow, water=blue, grass=green, etc.). CLI gains `--color` flag (`auto`/`on`/`off`; default `auto` detects TTY). Zero new dependencies (raw ANSI escape codes).
- **Phase 5.7 — Rest Points & Seed Reporting**: Role-based tension caps create pacing valleys in composed maps. Sub-pattern Entry nodes are converted to **Transition** during grafting (`compose.rs`) — they're passages into sub-areas, not dungeon entrances. **Transition rooms** capped at Medium — passages provide breathing room. **Hub rooms** capped at High — crossroads, not boss chambers. Gate and Branch remain uncapped (gates ARE the challenge). Only the root Entry (depth 0) is forced Low. **Rest point insertion** (`src/intent/rest_points.rs`): after composition, if the critical path > 5 edges and lacks a natural Low room mid-path, a "Safe Passage" Transition node with `rest_point` structural tag is inserted by splitting a Traversal edge near the midpoint. The tension system forces `rest_point`-tagged rooms to Low. Result: composed cave maps show proper tension curves (Low → Medium → High → Climax → Low → Medium → Medium → Medium → Climax) with Transition caps providing natural valleys. `PipelineResult.seed: u64` field stores the actual seed used (generated from entropy when not specified). CLI always prints `(seed: N)` — users can reproduce any map with `--seed N`. 4 new unit tests in `tension.rs` (11 total).
- **Phase 5.8 — Pacing Validator**: `PacingValidator` in `src/validate/pacing.rs` checks the tension curve for degenerate patterns. 4 checks: flat curve (all rooms same tension), no climax (no dramatic peak), immediate climax (Climax at depth 1 with long map), no rest on long critical path. All produce Warnings/Infos (never Errors — bad pacing doesn't fail generation). `PipelineResult.pacing_warnings: Vec<String>` carries issues to the world engine. CLI prints ⚠ warnings when present. 8 unit tests for the validator.
- **Phase 6.1 — Urban Space Archetypes & Edge Zones**: New `SpaceArchetype` variants (`Plaza`, `Street`, `Alley`, `Shop`, `Warehouse`). `LocationKind::Urban` added. `ZoneKind` extended with `EdgeZone` (periphery of urban spaces where buildings/stalls attach) and `Frontage` (street-facing strip of building parcels). Zone classification (`classify_zones`) gains optional `archetype` parameter — when urban archetypes are present, cells are classified using urban-specific rules: streets get linear edge zones along long edges, plazas get all-edge zones, shops/warehouses get frontage on the street-facing wall. Spatial planner gains dimension resolution for urban archetypes (elongated streets, square plazas).
- **Phase 6.2 — Multi-Connector Routing & Distribution**: `ConnectorDistribution` enum (`Uniform`, `Clustered`, `GridAligned`, `Ends`). `SpaceSpec` gains `max_connectors: Option<u32>` and `connector_distribution: Option<ConnectorDistribution>`. Helper methods `effective_max_connectors()` and `effective_connector_distribution()` provide archetype-aware defaults (Street=10/Uniform, Plaza=8/Clustered, Corridor/Alley=4/Ends, Warehouse=4/GridAligned). 16 unit tests for connector behavior.
- **Phase 6.3 — Alignment-Aware Geometry**: `AlignSide` enum (`North`/`South`/`East`/`West`), `Axis` enum (`Horizontal`/`Vertical`), 3 new `SpatialConstraint` variants (`AlignEdge`, `AttachToEdge`, `PreferOrientation`). `derive_urban_constraints()` auto-infers orientation+attachment constraints for `LocationKind::Urban`. `StreetSkeletonPlanner` — new `GeometryPlanner` impl in `src/geometry/street_skeleton.rs`: partitions spaces into backbone (Street/Alley/Plaza) vs attached (buildings), places backbone axis-aligned first, attaches buildings flush against backbone edges, resolves overlaps. `GeometryStrategy::StreetSkeleton` variant + auto-select for Urban. CLI `--layout street`. Force-directed planner gains orientation-aware snap-to-grid (swaps dimensions to match `PreferOrientation`). 5 unit tests for street skeleton planner.

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
├── tension.rs           # TensionLevel enum, compute_tension() (BFS-based pacing curve)
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
│   ├── pattern.rs       # NarrativePattern, PatternSlot, PatternEdge, PatternVote, ExpansionVote
│   ├── selector.rs      # score_pattern, rank_patterns, select_pattern (weighted voting + RNG tiebreak)
│   ├── vocabulary.rs    # ThemeVocabulary, RoleVocabulary, VocabularyEntry
│   ├── filler.rs        # fill_pattern: slot instantiation with vocabulary + RNG
│   ├── compose.rs       # fill_pattern_composed: recursive sub-pattern expansion + graph grafting
│   ├── rest_points.rs   # insert_rest_points: post-composition critical path analysis + node insertion
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
│   ├── street_skeleton.rs # StreetSkeletonPlanner (backbone-first urban layout)
│   ├── routing.rs       # CorridorRouter trait + ZShapeRouter (face-based routing with corridor merging)
│   └── shape.rs         # ShapeRefinement trait + RectShape (no-op) + CaveIrregularizer (organic cave shapes)
├── tile/                # Tile-level rasterization
│   ├── registry.rs      # TileId newtype, Tile constants, TileProperties, TileRegistry
│   ├── map.rs           # TileMap (stores Vec<TileId>), flood_fill
│   ├── rasterize.rs     # GeometryPlan → TileMap (trait + impl + proptest)
│   ├── scatter.rs       # TileScatterRule, apply_scatter (tag→tile replacement with density, reserved-path aware)
│   ├── ascii.rs         # TileMap → String debug rendering (+ feature + entity overlay)
│   └── color.rs         # ANSI-colored ASCII rendering (danger/importance coloring)
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
│   ├── registry.rs      # EntityCategory enum, EntityProperties, EntityRegistry
│   ├── rules.rs         # EntityRule, EntityPlacementStrategy, default_entity_rules(), matching_entity_rules()
│   └── planner.rs       # EntityPlanner trait + SimpleEntityPlanner impl
├── validate/            # Validation traits
│   ├── mod.rs           # Validator<T> trait, Severity, ValidationResult
│   ├── geometry.rs      # GeometryValidator
│   ├── feature.rs       # FeatureValidator
│   ├── entity.rs        # EntityValidator
│   └── pacing.rs        # PacingValidator (tension curve quality checks)
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
    ├── entity_types.json # Entity type registry (name→category+char+blocking+tags)
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
cargo run -- --annotate            # Room letters on map + legend below (with tension levels)
cargo run -- --color on            # Force ANSI colored output
cargo run -- --color off           # Plain text (no escape codes)
cargo run -- --color auto          # Auto-detect terminal (default)
cargo run -- --layout column       # Use BFS column layout (original)
cargo run -- --layout force        # Use force-directed layout (default)
cargo run -- --layout street       # Use street-skeleton layout (urban)
cargo run -- --trace               # INFO-level pipeline trace (to stderr)
cargo run -- --trace debug         # DEBUG-level (placement details, routing decisions)
cargo run -- --trace all           # TRACE-level (everything)
cargo run -- --help                # Show CLI usage
RUST_LOG=procgen=debug cargo run -- --trace  # Override via env var
cargo test       # Runs all tests (487 currently: 383 unit/proptest + 23 regression + 77 integration + 4 doctests)
```

## Target Architecture

See `PLAN.md` for the full evolution plan from current state to the target layered architecture.
