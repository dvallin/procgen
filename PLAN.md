# PLAN.md — Architecture Evolution Plan

## Vision

Evolve the current thin vertical slice into a layered procedural generation pipeline where each layer has clear contracts, swappable implementations, and validation hooks that enable iterative refinement.

---

## Target Architecture

```
SituationContext
│   narrative tags, world bindings
│   "port town house, rat infestation, dock district"
│
▼
MapIntent
│   location_kind, scale, required_spaces, structural_graph
│   constraints, motifs
│
▼
SpatialPlan
│   abstract spaces: roles, archetypes, size_hints
│   links: traversal semantics
│   spatial constraints: adjacency, separation, ordering
│
├─────────────────────────────┐
▼                             ▼
GeometryPlan                  │  (consumes SpatialPlan + MapIntent)
│   placed spaces with        │
│   footprints (rect, cells,  │
│   composite)                │
│   placed links as polylines │
│                             │
▼                             │
TileMap  ◄────────────────────┘  (consumes GeometryPlan)
│   2D grid: floor, wall, door, void, water, ...
│
▼
FeaturePlan (consumes: MapIntent + GeometryPlan + TileMap)
│   furniture, traps, decorations, interactables
│   placed with footprints + anchors + tags
│
▼
EntityPlan (consumes: FeaturePlan + TileMap + MapIntent)
│   NPCs, monsters, items, triggers
│   positioned with behavior tags
│
▼
Validation (consumes: all outputs)
│   connectivity, reachability, density, pacing
│   failure → adjust constraints → rerun affected layers
│
▼
Presentation / Debug / Export
```

### Data Flow Rules

- **Down is default.** Each layer's output struct is the input for the next.
- **Cross-references are explicit.** FeaturePlan receives MapIntent + GeometryPlan + TileMap as explicit inputs.
- **Validators trigger reruns, not mutations.** On failure, constraints are adjusted and the offending layer re-executes.

---

## Layer Definitions

### SituationContext

The world/narrative layer. Provides thematic context without prescribing map structure.

```rust
struct SituationContext {
    tags: Vec<Tag>,
    bindings: HashMap<String, Value>,
}
```

Example: `tags: ["port_town", "rat_infestation", "food_shortage", "dock_district"]`

This layer does not need to understand *why* or *what it means*. The next layer translates world facts into map-relevant directives.

---

### MapIntent

Translates narrative context into map-level directives. This is the "what should this map contain and feel like" layer.

```rust
struct MapIntent {
    location_kind: LocationKind,
    scale: MapScale,
    tags: Vec<Tag>,
    motifs: Vec<MotifId>,
    required_spaces: Vec<SpaceRequirement>,
    structural_graph: StructuralGraph,
    constraints: Vec<IntentConstraint>,
}
```

The `structural_graph` subsumes the current `ScenarioGraph` — it describes rooms, their roles, and traversal relationships. The rest of `MapIntent` adds non-structural requirements (scale, motifs, constraints).

---

### SpatialPlan

Abstract spatial topology. Knows *what* spaces exist and how they relate, but not *where* they are.

```rust
struct SpatialPlan {
    spaces: Vec<SpaceSpec>,
    links: Vec<SpaceLink>,
    constraints: Vec<SpatialConstraint>,
}

struct SpaceSpec {
    id: SpaceId,
    role: SpaceRole,
    archetype: Option<SpaceArchetype>,
    tags: Vec<Tag>,
    size_hint: SizeHint,
}

struct SpaceLink {
    from: SpaceId,
    to: SpaceId,
    traversal: TraversalKind,
    tags: Vec<Tag>,
}

enum SizeHint {
    Tiny,       // 3x3 – 5x5
    Small,      // 5x5 – 7x7
    Medium,     // 7x7 – 11x9
    Large,      // 11x9 – 15x13
    Custom { min_w: i32, min_h: i32, max_w: i32, max_h: i32 },
}
```

This replaces the current `FragmentGraph`. The key addition is `SpatialConstraint` — rules like "key_room must not be adjacent to goal" or "hub must be reachable within 2 links from entry".

---

### GeometryPlan

The first concrete spatial layer. Assigns coordinates and shapes to abstract spaces.

```rust
struct GeometryPlan {
    spaces: Vec<PlacedSpace>,
    links: Vec<PlacedLink>,
}

struct PlacedSpace {
    id: SpaceId,
    footprint: Footprint,
    zones: Vec<Zone>,
    tags: Vec<Tag>,
}

enum Footprint {
    Rect(Rect),
    Cells(Vec<Point>),
    Composite(Vec<Footprint>),
}

struct Zone {
    role: ZoneRole,
    cells: Vec<Point>,
}
```

`Footprint::Cells` enables irregular shapes (caves, L-shaped rooms). `Zone` subdivides a space into functional regions (e.g. "altar area" within a chapel).

This replaces the current `PlacedLayout`.

---

### TileMap

The ground-truth 2D grid. Everything downstream reads from this.

```rust
struct TileMap {
    width: u32,
    height: u32,
    tiles: Vec<Tile>,
}

enum Tile {
    Void,
    Floor,
    Wall,
    Door,
    LockedDoor,
    Water,
    Pit,
    Stairs,
    // ... extensible
}
```

Stays largely as-is. Tile variants will grow over time.

---

### FeaturePlan

Objects placed *on* the tile map. Needs map intent (room purpose), geometry (spatial context), and tiles (walkability).

```rust
struct FeaturePlan {
    features: Vec<FeaturePlacement>,
}

struct FeaturePlacement {
    kind: FeatureKind,
    footprint: Vec<Point>,
    anchor: Point,
    space_id: SpaceId,
    tags: Vec<Tag>,
}

enum FeatureKind {
    Furniture(FurnitureType),
    Container(ContainerType),
    Trap(TrapType),
    Decoration(DecorationId),
    Interactable(InteractableId),
}
```

---

### EntityPlan

Dynamic things with behavior. Placed after features so they don't conflict.

```rust
struct EntityPlan {
    entities: Vec<EntityPlacement>,
}

struct EntityPlacement {
    archetype: EntityArchetypeId,
    position: Point,
    space_id: SpaceId,
    behavior_tags: Vec<Tag>,
    patrol_zone: Option<Vec<Point>>,
}
```

---

### Validation

Generic trait that any layer output can implement validators for.

```rust
trait Validator<T> {
    fn validate(&self, value: &T, context: &ValidationContext) -> ValidationResult;
}

struct ValidationResult {
    issues: Vec<ValidationIssue>,
}

struct ValidationIssue {
    severity: Severity,
    kind: IssueKind,
    message: String,
    suggestion: Option<ConstraintAdjustment>,
}

enum Severity {
    Error,   // must fix — rerun
    Warning, // acceptable but suboptimal
    Info,    // debug information
}
```

Validators are registered per-layer. The pipeline runner checks after each stage and decides whether to retry.

---

## Migration Mapping

| Old | New | Status |
|---|---|---|
| `scenario/graph.rs` → `ScenarioGraph` | `intent/graph.rs` → `ScenarioGraph` (will become `MapIntent.structural_graph`) | ✅ renamed |
| `scenario/template.rs` | `intent/template.rs` | ✅ moved |
| `scenario/instantiate.rs` | `intent/instantiate.rs` | ✅ moved |
| `fragment/graph.rs` → `FragmentGraph` | `spatial/plan.rs` → `SpatialPlan` | ✅ renamed |
| `fragment/expand.rs` → `FragmentExpander` | `spatial/planner.rs` → `SpatialPlanner` | ✅ renamed |
| `layout/geom.rs` → `PlacedLayout` | `geometry/geom.rs` → `GeometryPlan` | ✅ renamed |
| `layout/place.rs` → `Placer` | `geometry/planner.rs` → `GeometryPlanner` | ✅ renamed |
| `tile/` | `tile/` (unchanged) | ✅ |
| *(new)* | `feature/` | ✅ stub |
| *(new)* | `entity/` | ✅ stub |
| *(new)* | `validate/` | ✅ stub |
| *(new)* | `situation/` | ✅ implemented |
| *(new)* | `intent/map_intent.rs` | ✅ implemented |
| *(new)* | `intent/builder.rs` | ✅ implemented |

---

## Phased Task Breakdown

### Phase 1: Foundation — Tag Newtype + Module Restructure ✅

**Goal:** Align module names with target architecture. No logic changes.

**Status: COMPLETE**

| # | Task | Status |
|---|---|---|
| 1.1 | Introduce `Tag` newtype in `src/tag.rs`, re-export from `lib.rs` | ✅ |
| 1.2 | Migrate all `Vec<String>` tag fields to `Vec<Tag>` | ✅ |
| 1.3 | Rename `scenario/` → `intent/` | ✅ |
| 1.4 | Rename `fragment/` → `spatial/`, types renamed (`SpatialPlan`, `SpaceSpec`, etc.) | ✅ |
| 1.5 | Rename `layout/` → `geometry/`, types renamed (`GeometryPlan`, `PlacedSpace`, etc.) | ✅ |
| 1.6 | Add stub modules: `feature/`, `entity/`, `validate/`, `situation/` | ✅ |
| 1.7 | Add utility methods: `Point::cardinals/neighbors`, `Rect::overlaps`, `Tile::is_walkable/is_solid`, `TileMap::flood_fill` | ✅ |
| 1.8 | Property-based tests for spatial planner (5 tests) and rasterizer (3 tests) | ✅ |
| 1.9 | Integration tests for the crypt demo (connectivity, overlaps, tile types, space count) | ✅ |
| 1.10 | Verify `cargo run` and `cargo test` pass with zero warnings | ✅ |

**Exit criterion:** Same ASCII output, new module names, `Tag` newtype in use, 12 tests passing.

---

### Phase 2: MapIntent + SituationContext ✅

**Goal:** Introduce the upper layers of the pipeline.

**Status: COMPLETE**

| # | Task | Status |
|---|---|---|
| 2.1 | Define `SituationContext` struct in `situation/` with tags + bindings + builder methods | ✅ |
| 2.2 | Define `MapIntent` struct in `intent/map_intent.rs` (LocationKind, MapScale, MotifId, IntentConstraint) | ✅ |
| 2.3 | Implement `trait IntentBuilder` in `intent/builder.rs` (`SituationContext → MapIntent`) | ✅ |
| 2.4 | Refactor `crypt.rs` demo: `build_crypt_situation()` + `CryptIntentBuilder` | ✅ |
| 2.5 | Change `SpatialPlanner::plan` to accept `&MapIntent` instead of `&ScenarioGraph` | ✅ |
| 2.6 | Update `main.rs` to use full pipeline: situation → intent → spatial → geometry → tiles → ascii | ✅ |
| 2.7 | Update integration tests to use Phase 2 path + new metadata test | ✅ |
| 2.8 | Verify `cargo run`, `cargo test`, `cargo clippy` all pass with zero warnings | ✅ |

**Exit criterion:** Pipeline starts from `SituationContext`, flows through `MapIntent`, same ASCII output, 13 tests passing.

---

### Phase 3: Enrich SpatialPlan ✅

**Goal:** Make the spatial layer expressive enough for diverse maps.

**Status: COMPLETE**

| # | Task | Status |
|---|---|---|
| 3.1 | Add `SpaceArchetype` enum (Hall, Chamber, Corridor, Vestibule, Vault, Shaft, Courtyard, Workshop) | ✅ |
| 3.2 | Add `SizeHint` enum to `SpaceSpec` (Tiny, Small, Medium, Large, Grand, Custom) | ✅ |
| 3.3 | Add `SpatialConstraint` enum (MustBeAdjacent, MustBeSeparated, MaxDistance, GatedBy, PreferPerimeter, PreferCentral) | ✅ |
| 3.4 | Update `SpatialPlanner` to use archetypes + hints via `resolve_dimensions()` | ✅ |
| 3.5 | Add tavern cellar demo (`demo/tavern.rs`) with different topology + archetypes | ✅ |
| 3.6 | Replace hardcoded geometry planner with BFS-based generic layout | ✅ |
| 3.7 | Z-shape corridor routing with room-aware safe transfer coordinate search | ✅ |
| 3.8 | Straight corridors when rooms share y/x range; Z-shape only when needed | ✅ |
| 3.9 | Door connectivity tests (doors must have 2+ walkable cardinal neighbors) | ✅ |
| 3.10 | Both demos produce valid, connected output (30 tests passing) | ✅ |

**Exit criterion:** Two demos working, spatial plan carries archetype/constraint information, geometry planner is generic with room-avoiding corridors.

---

### Phase 4: Real Geometry Placement

**Goal:** Improve placement algorithm quality, add validation, and wire up a retry loop.

**Status: IN PROGRESS**

**Context:** Phase 3 delivered a generic BFS-based layout with Z-shape corridor routing that avoids room interiors. Phase 4 adds: the `Footprint` abstraction for non-rectangular shapes, extraction of routing as a swappable trait, constraint-aware placement, formal validation, and retry loops. Phase 4.3 also promoted geometry helpers (`point_on_boundary`, `inflate`, `gap_to`, `segment_crosses_interior`) onto `Rect` in `geometry/geom.rs` so downstream code (validator, routing) uses methods on the type rather than free functions.

| # | Task | Description | Status |
|---|---|---|---|
| 4.1 | Introduce `Footprint` enum (Rect, Cells, Composite) + update data model | `geometry/geom.rs` — `Footprint` with `bounding_rect()`, `PlacedSpace.footprint` field, rasterizer dispatches on variant | ✅ |
| 4.2 | Extract corridor routing into `geometry/routing.rs` with `trait CorridorRouter` | `ZShapeRouter` impl, 3 unit tests (straight H, straight V, Z-shape) | ✅ |
| 4.3 | Implement `GeometryValidator` | `validate/geometry.rs` — overlap check, minimum spacing, link endpoint validity, link connectivity, corridor–room intersection check; 18 unit tests + 5 proptests. Also added `Rect::point_on_boundary`, `Rect::inflate`, `Rect::gap_to`, `Rect::segment_crosses_interior` to `geometry/geom.rs` (15 unit tests). | ✅ |
| 4.4 | Make placement constraint-aware | `geometry/planner.rs` — respect `PreferCentral`, `PreferPerimeter`, `MustBeSeparated`; add minimum gap enforcement post-placement | |
| 4.5 | Improve corridor routing robustness | `geometry/routing.rs` — detect corridors through non-endpoint rooms, try alternate transfer points, L-shape fallback, last-resort pass-through | |
| 4.6 | Wire validator into pipeline + retry logic | `geometry/planner.rs` — `plan_with_validation()` that retries with relaxed spacing on `Severity::Error`; update `main.rs` | |
| 4.7 | Stress test + final integration | Proptest: random `SpatialPlan`s (3–10 spaces), assert validation passes + rasterizer produces connected maps; fix fuzzer-found issues | |

**Design decisions for Phase 4:**

- **Evolve, don't replace.** The BFS tree layout stays as the core algorithm. Constraint awareness is added as adjustments to the existing placement pass, not a new solver.
- **Validate after, not during.** The `GeometryValidator` runs post-generation. This keeps the planner simple and makes validation independently testable.
- **Retry is simple.** On failure: increase `DEPTH_SPACING` and `SIBLING_SPACING` by 2, retry up to 3 times. No complex backtracking.
- **Routing extensibility via trait.** `CorridorRouter` is a trait so future Phase 8 work (A*, meandering corridors) can be dropped in without touching placement.
- **Footprint is forward-looking.** Only `Footprint::Rect` is used in Phase 4. `Cells` and `Composite` exist as extension points for Phase 8 (caves, L-shaped rooms).

**Parallelization notes:**
- 4.1 + 4.2 were independent (different files) → done in parallel ✅
- 4.3 + 4.4 are mostly independent (4.3 writes `validate/geometry.rs`, 4.4 modifies `geometry/planner.rs`) → can be parallelized
- 4.5 depends on 4.3 (needs validation to see what's broken)
- 4.6 depends on 4.3 + 4.4 + 4.5 (orchestrates them)
- 4.7 depends on 4.6 (stress-tests the full loop)

**Exit criterion:** Placement is algorithmic, constraint-aware, validated, and produces reasonable output for both demos. 72+ tests passing including geometry validation.

---

### Phase 5: FeaturePlan

**Goal:** Place objects within rooms based on intent and geometry.

| # | Task | Description |
|---|---|---|
| 5.1 | Define `FeaturePlan`, `FeaturePlacement`, `FeatureKind` structs | `feature/plan.rs` |
| 5.2 | Define `trait FeaturePlanner` | takes `MapIntent + GeometryPlan + TileMap → FeaturePlan` |
| 5.3 | Simple implementation: place features by room role (altar in chapel, chest in vault) | `feature/planner.rs` |
| 5.4 | Extend tile rendering to show features (new chars or overlay) | `tile/ascii.rs` |
| 5.5 | Validate: no features on walls, required features present per role | `validate/feature.rs` |

**Exit criterion:** Demo output shows placed features, validation passes.

---

### Phase 6: EntityPlan

**Goal:** Place dynamic entities (monsters, NPCs, items).

| # | Task | Description |
|---|---|---|
| 6.1 | Define `EntityPlan`, `EntityPlacement`, `EntityArchetypeId` structs | `entity/plan.rs` |
| 6.2 | Define `trait EntityPlanner` | takes `FeaturePlan + TileMap + MapIntent → EntityPlan` |
| 6.3 | Simple implementation: spawn by room tags + density rules | `entity/planner.rs` |
| 6.4 | Extend rendering to show entities | `tile/ascii.rs` |
| 6.5 | Validate: entities on walkable tiles, not on features, density within bounds | `validate/entity.rs` |

**Exit criterion:** Demo output shows entities, validation passes.

---

### Phase 7: Pipeline Runner + Validation Loop

**Goal:** Formalize the generation pipeline as a reusable runner with retry semantics.

| # | Task | Description |
|---|---|---|
| 7.1 | Define `Pipeline` struct that orchestrates all stages | `pipeline.rs` |
| 7.2 | Define `PipelineConfig` with max retries, constraint relaxation strategy | `pipeline.rs` |
| 7.3 | Implement retry loop: on validation failure, adjust constraints and rerun | `pipeline.rs` |
| 7.4 | `main.rs` becomes a thin wrapper around `Pipeline::run(situation)` | `main.rs` |
| 7.5 | Add integration test that runs pipeline N times and asserts all outputs valid | `tests/` |

**Exit criterion:** Pipeline runner handles failures gracefully, multiple runs all produce valid output.

---

### Phase 8: Algorithmic Depth (open-ended)

**Goal:** Now flesh out individual layers with sophisticated algorithms.

| Area | Possible approaches |
|---|---|
| Spatial planning | Graph grammars, L-systems, template expansion |
| Geometry placement | BSP subdivision, Poisson disc, force-directed, constraint solving |
| Room shapes | Cellular automata (caves), Perlin noise, prefab templates, WFC |
| Corridors | A* with cost maps, river-style meandering, multi-width |
| Features | Rule-based placement, template interiors, WFC for furniture layout |
| Entities | Encounter budgets, patrol path generation, loot tables |
| Validation | Reachability analysis, pacing curves, difficulty gradients |

Each of these is an independent implementation behind an existing trait — safe to explore without breaking the architecture.

---

## Design Decisions & Rationale

### Why separate SpatialPlan from GeometryPlan?

Spatial planning is *topological* (what connects to what, how big roughly). Geometry planning is *metric* (exact coordinates, shapes). Separating them means you can:
- Change placement algorithms without changing the abstract room graph
- Validate spatial relationships before committing to coordinates
- Support different geometry strategies (grid-aligned, free-form, template-based) for the same spatial plan

### Why is FeaturePlan after TileMap?

Features need to know walkable cells (from TileMap), room boundaries (from GeometryPlan), and room purpose (from MapIntent). They're placed *on* terrain, not *as* terrain.

### Why separate FeaturePlan from EntityPlan?

Features are static (furniture, traps, decorations). Entities are dynamic (monsters, NPCs). They have different placement constraints:
- Features: footprint-based, can block movement, define room character
- Entities: point-based, must be on walkable tiles, need patrol zones

### Why validators instead of generation-time checks?

Validators are composable, testable, and can be run independently. They also enable the retry loop: "this layout failed connectivity validation, relax spacing constraints and try again." Baking validation into generators makes them harder to test and combine.

### Why top-down?

Because the alternative (bottom-up from tile generation) leads to:
1. Over-investment in one layer before knowing what consumers need
2. Refactoring pain when you discover layer N+1 needs different data from layer N
3. The classic "I spent a month on cave generation but now I can't place doors" problem

Top-down means contracts are stable before implementations get complex.

---

## Success Criteria

The architecture is "done" when:

1. A single `Pipeline::run(SituationContext)` call produces a fully populated map with rooms, corridors, features, and entities.
2. Multiple demo scenarios (crypt, tavern, cave system, mansion) all produce valid, distinct output.
3. Any single layer can be replaced with a new implementation without touching other layers.
4. Validation catches obviously broken maps and the retry loop fixes them.
5. `cargo run` always works.
