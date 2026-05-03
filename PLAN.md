# PLAN.md — Roadmap

## Vision

A story-driven procedural generator where narrative signals steer every decision. Given a `SituationContext` ("sealed crypt, noble family, locked vault"), the system infers structure, materializes themed rooms, places narratively appropriate features and entities, and produces a map that feels *designed* — with pacing, tension, and coherence — without any hand-authored graph.

---

## Where We Are

The pipeline skeleton is complete and validated. Three demo scenarios (crypt, tavern, cave) flow through all layers and produce ASCII output. 309 tests (property-based + integration + regression + doctests) confirm layer invariants hold for arbitrary inputs. Seeded RNG ensures reproducibility. **Phases 1–3 are complete.**

```
SituationContext → IntentBuilder → MapIntent → SpatialPlan → GeometryPlan → TileMap (+scatter) → FeaturePlan → EntityPlan → ASCII
```

**What works:**
- Full pipeline runner with retry semantics and seeded RNG
- Data-driven intent generation: pattern selection → vocabulary lookup → slot filling → constraint inference → MapIntent assembly
- Geometry placement with constraint-aware layout and corridor routing
- Feature placement (furniture, traps, decorations) driven by role/tag rules
- Entity placement (monsters, NPCs) with patrol zones and safe entry rooms
- Validators at geometry, feature, and entity levels
- Property-based stress tests with random spatial plans
- Two regression scenarios: Noble Crypt (lock-and-key), Tavern Cellar (hub-and-spoke)
- Data-driven rules: Feature/entity rules loaded from JSON assets
- Asset loader: Generic `load_rules<T>()` with file + embedded fallback
- Pipeline rule overrides: `PipelineConfig.feature_rules` / `.entity_rules` let callers inject custom rule sets
- 5 narrative patterns as JSON, pattern selector with weighted voting + RNG tiebreaking
- 2 theme vocabularies with role-indexed entries (labels, tags, archetypes, location_kind, motifs)
- `Pipeline::run()` uses `GenericIntentBuilder` internally; no hard-coded builders remain

- Tile registry: `TileId(u16)` + `TileRegistry` with JSON-defined tile types (walkable, opaque, ascii_char, tags)
- Extended palette: Water, Pit, Stairs, Rubble, Grass — all as real tiles in the `TileMap`
- Tile scatter rules: data-driven floor→terrain replacement during rasterization (connectivity-safe for non-walkable tiles)
- Registry-aware ASCII renderer (`render_ascii_full`)
- Proper tiles/features separation: tiles ARE the ground; features are objects ON tiles

**What's next (Phase 4):**
- Narrative Room Character & Atmosphere

---

## Design Boundary: Types vs. Data

A core tension in this system: what stays as Rust enums/traits (compile-time, exhaustive) vs. what becomes data-driven (runtime, open-ended)?

**Stays typed (structural contracts):**
- `NodeRole` — the finite set of *structural functions* a room can serve (Entry, Hub, Gate, Goal, Reward, Branch, Transition). These are architectural concepts, not content.
- `EdgeRole` — traversal semantics determine corridor generation logic.
- Pipeline traits (`IntentBuilder`, `SpatialPlanner`, etc.) — the layer boundaries.
- `Severity` — validators need exhaustive match arms.
- `Tile` — *for now*. See Phase 3 for the path to data-driven tiles.

**Becomes data (content/variety):**
- Feature/entity rules — which features spawn in which rooms, driven by tags
- Theme vocabularies — labels, tags, atmosphere, archetype selection per theme
- Narrative patterns — reusable graph structures (lock-and-key, hub-and-spoke)
- Room templates — pre-designed interior layouts for specific archetype+theme combos
- Tile palettes — *eventually*. A "tile registry" maps tile IDs to properties (walkable, opaque, ASCII char). But the MVP can keep the enum.

**The heuristic:** If adding a new *kind* of X requires changing match arms in multiple files, X should probably be data. If X determines which *code path* executes, it should stay typed.

---

## Phased Roadmap

### Phase 1: Data-Driven Rules & Asset Loading ✅

**Goal:** Extract hard-coded feature/entity rules into JSON assets. The rule engine becomes generic — rules are data, matching logic is code.

| # | Task | Status | Description |
|---|---|---|---|
| 1.1 | **FeatureRule JSON schema** | ✅ | Serde derives on `FeatureRule`, `FeatureKind`, `PlacementStrategy`, `NodeRole`, `SpaceArchetype`, `Tag`. Round-trip tests. |
| 1.2 | **EntityRule JSON schema** | ✅ | Serde derives on `EntityRule`, `EntityPlacementStrategy`, `EntityArchetypeId`. Round-trip tests. |
| 1.3 | **Asset loader** | ✅ | `load_rules<T>()`, `load_rules_from_str<T>()`, `load_rules_with_fallback<T>()`. `AssetLoadError` with Io/Parse variants. Embedded defaults via `include_str!`. |
| 1.4 | **Default rule assets** | ✅ | `assets/rules/features.json` (10 rules) + `assets/rules/entities.json` (7 rules). `default_rules()` functions are thin loaders. |
| 1.5 | **Pipeline accepts rule sets** | ✅ | `PipelineConfig.feature_rules` / `.entity_rules` overrides. Planner traits accept `rules: &[Rule]` parameter. |
| 1.6 | **Proptest: random rules** | ✅ | 4 property-based tests generate arbitrary rule sets (0–10 rules, random roles/tags/counts) and confirm no panics + valid output. |

**Exit criterion met:** `cargo run` produces identical output with rules loaded from JSON. Users can modify JSON and see different feature/entity behavior without recompiling. Random rules never panic.

**Future integration:** Rule sets are currently loaded as a pipeline-level concern (`PipelineConfig`). In the future, a **world layer** above `SituationContext` will select which rule files to load based on regional themes (e.g., undead region → `undead_crypts.json`, port city → `urban_cellars.json`). The seam is already clean — `PipelineConfig` is where a world layer injects its choices.

---

### Phase 2: Narrative Patterns & Intent Generation ✅

**Goal:** Replace hard-coded `IntentBuilder` fixtures with a generative system that infers a complete `MapIntent` from just situation tags + bindings.

**Key insight:** Patterns encode *only topology* (slots + edge roles + votes). Everything else—constraints, location kind, scale, motifs—is *inferred* from the filled graph structure and theme vocabulary. A pattern author never writes redundant constraint declarations; the system derives them from the topology.

| # | Task | Status | Description |
|---|---|---|---|
| 2.1 | **NarrativePattern data model** | ✅ | A pattern is a parameterized graph template: named slots with role constraints, edges with traversal types, vote metadata. JSON-serializable. Patterns declare topology only—no explicit constraint arrays for structurally-inferable invariants. |
| 2.2 | **Pattern library** | ✅ | 5 starter patterns as JSON asset with embedded fallback: lock-and-key, hub-and-spoke, linear-descent, gauntlet, branching-exploration. |
| 2.3 | **Pattern Selector** | ✅ | Situation tags have weighted votes for patterns. Highest-scoring pattern wins (with RNG tiebreaking). |
| 2.4 | **Theme Vocabulary** | ✅ | Maps `(role, theme)` → list of `{label, tags, archetype}` tuples. Two starter themes (undead_nobility, urban_underground). Also carries theme-level metadata: `location_kind` and `motifs`. |
| 2.5 | **Slot Filler** | ✅ | Given a pattern + vocabulary, instantiate each slot: pick a vocabulary entry matching the slot's role, assign label/tags/archetype. Optional slots included via RNG. Returns a ScenarioGraph (no constraints). |
| 2.6 | **Structural Constraint Inference** | ✅ | Scan the *filled ScenarioGraph* and derive constraints from topology: Gate-role node + RestrictedTraversal outgoing edge → emit `MustGate`. Required node off Hub via Traversal → emit `MustConnect`. Graph depth from Entry → emit `MaxDepth`. Pattern-level `max_depth` override honored when present. |
| 2.7 | **GenericIntentBuilder** | ✅ | Implement `IntentBuilder` that orchestrates: (a) select pattern, (b) look up vocabulary from `bindings["theme"]`, (c) fill slots, (d) infer constraints from filled graph, (e) infer `location_kind` + `scale` + `motifs` from vocabulary metadata + filled node count. One builder for all situations. |
| 2.8 | **Regression tests** | ✅ | Given crypt/tavern situations, `GenericIntentBuilder` produces graphs with the same structural shape (same role counts, same edge types) as the fixture builders. Constraints match structural expectations. `main.rs` wired to use `GenericIntentBuilder` for all scenarios. |

**Design decisions:**
- Patterns are *topological*, not geometric. They define what connects to what, not where.
- Patterns declare **only** non-inferable metadata: `max_depth` override, votes. Structurally-obvious constraints (`MustGate`) are auto-derived from the filled graph.
- Theme vocabularies are the primary variety knob. Adding a scenario = adding vocabulary entries + situation tags.
- The vocabulary also provides `location_kind` and `motifs` so the builder doesn't need heuristics for those.
- `scale` is inferred from the filled graph size (node count mapped to MapScale).
- Existing demo fixtures (`CryptIntentBuilder`, `TavernIntentBuilder`) were removed after regression tests confirmed the generic builder produces equivalent output. Demo files now contain only situation factories.

**Exit criterion:** `cargo run` with a `GenericIntentBuilder` + crypt situation produces a valid, thematically coherent dungeon indistinguishable in quality from the fixture version.

---

### Phase 3: Tile Registry & Data-Driven Rasterization ✅

**Goal:** Break the hard-coded `Tile` enum into a registry-based system so game-specific terrain can be defined in data.

| # | Task | Status | Description |
|---|---|---|---|
| 3.1 | **TileId newtype** | ✅ | `TileId(u16)` — a lightweight handle into the tile registry. `TileMap` stores `Vec<TileId>` instead of `Vec<Tile>`. |
| 3.2 | **TileRegistry** | ✅ | Maps `TileId` → `TileProperties { name, walkable, opaque, ascii_char, tags }`. Loaded from JSON. Built-in defaults cover the current enum variants. |
| 3.3 | **Migrate Tile enum → TileId** | ✅ | `Tile::Floor` becomes `TileId(1)` etc. All code that pattern-matches on `Tile` now queries registry properties (`registry.is_walkable(id)`). |
| 3.4 | **Rasterizer uses registry** | ✅ | `SimpleRasterizer` receives `&TileRegistry` (via pipeline config or trait method). Uses `registry.is_walkable()` in wall inference. |
| 3.5 | **Extended tile palette** | ✅ | Added tiles for Water, Pit, Stairs, Rubble, Grass — each with properties. Cave scenario demonstrates terrain variety through the feature-layer overlay (Decoration subtypes). |
| 3.6 | **ASCII renderer uses registry** | ✅ | `render_ascii_full` and `render_ascii_with_registry` query `TileRegistry` for the display character. Backward-compat functions retain hardcoded fallback. |
| 3.7 | **Backward compat** | ✅ | `Tile` namespace struct with named constants (`Tile::FLOOR -> TileId(1)`) so existing tests need only mechanical renames. |
| 3.8 | **Tile scatter rules (JSON)** | ✅ | Data-driven rasterization-time tile scatter. `assets/rules/tile_scatter.json` maps room tags → tile replacements with density. The scatter step runs after rasterization and replaces `Floor` tiles with terrain variants (`Grass`, `Rubble`, `Water`) — producing actual `TileId`s in the `TileMap`. Non-walkable scatter is connectivity-safe (only places where all cardinal neighbors remain walkable). |

**Design decisions:**
- `NodeRole` and `EdgeRole` do NOT get this treatment — they determine which *code path* runs (corridor routing, entry room entity skip, etc.). They stay as enums.
- `TileId` is tiny (u16) and Copy — no performance regression vs. the enum.
- The registry is a game-level concern: different games using this generator can define different tile sets.
- Feature/entity placement strategies query `registry.is_walkable(tile)` instead of matching enum variants.
- **Tiles vs. Features:** Tiles are the ground itself (a cave room’s floor IS grass). Features are objects ON tiles (chests, altars). Terrain that affects movement/pathfinding should be a tile; cosmetic decorations that don’t block should be features. The tile scatter rules (3.8) handle the former; the feature rules handle the latter.
- **Tile scatter rules** follow the same pattern as feature/entity rules: JSON array of `{ match_tag, tile_name, density, avoid_doors }`. The rasterizer loads them via the asset system with embedded fallback. Walkable terrain tiles (Grass, Rubble) are safe to scatter freely; non-walkable terrain (Water, Pit) requires connectivity preservation (only scatter on tiles with ≥3 walkable cardinal neighbors).

**Exit criterion:** `cargo run -- cave` renders a map where the `TileMap` itself contains `Grass` and `Rubble` tile IDs (visible in ASCII output via registry lookup), AND feature decorations overlay on top. A new tile type can be added with zero Rust code changes — just JSON entries in `tiles.json` + `tile_scatter.json`.

---

### Phase 4: Motifs & Room Composition

**Goal:** Rooms gain spatial identity through constrained templates and zones. Features become data-driven like tiles. Motifs begin to influence structure, not just decoration. Tags gain classification so their intent is visible.

| # | Task | Description |
|---|---|---|
| 4.1 | **Feature Registry & FeatureCategory enum** | Mirror the tile registry pattern for features. Replace `FeatureKind` enum variants with a `feature_type: String` identifier + `FeatureRegistry` mapping type name → `FeatureProperties { category, ascii_char, blocking, tags }`. `FeatureCategory` enum (Furniture, Container, Trap, Decoration, Interactable) stays for gameplay behavior dispatch. Feature rules reference type strings ("altar", "chest") instead of enum variants. New feature types added via JSON without code changes. |
| 4.2 | **Tag classification & atmosphere profiles** | Split the flat `tags: Vec<Tag>` into typed fields on SpaceSpec: `structural_tags` (main_goal, locked — hard triggers), `atmosphere` (damp, echoing — soft flavor), `motifs` (rat_infestation — cross-cutting directives). Introduce **atmosphere profiles** (`assets/rules/atmospheres.json`): bundles of weighted scatter/feature/entity influences keyed by atmosphere combination. Rooms accumulate influences from matching profiles; the planner samples from the resulting palette. Replaces brittle one-off rules like `{ match_tag: "vast", feature: "stalagmite" }` with coherent taste bundles. |
| 4.3 | **Room zones & InteriorPlan** | Introduce `InteriorPlan { space_id, zones: Vec<Zone>, required_features: Vec<FeatureRequest>, reserved_paths: Vec<PathIntent> }`. Zones partition a room’s interior (center, wall-band, corners, door-path). Feature placement consults zones instead of raw tile queries. Reserved paths (door-to-door) are never blocked. Computed after GeometryPlan (needs concrete shapes), consumed by FeaturePlan. |
| 4.4 | **Template interiors (constraint-based)** | Not fixed prefabs — declarative templates with spatial constraints. E.g. `crypt_vault: { center: altar, wall-band: candles, clear: door-path }`. `tavern_storage: { wall-band: shelves, corners: barrels, center: clear }`. Templates produce an `InteriorPlan`; the feature planner respects it. |
| 4.5 | **Irregular room shapes** | Pull a minimal version of cave irregularity forward. `Footprint::Cells(Vec<Point>)` already exists — add an irregularizer that starts with a rect, carves corners, roughens edges, and preserves connectivity. Rooms with `LocationKind::Cave` or tag `"natural"` use it. Fixes the "office building under moss" look. |
| 4.6 | **MotifDirective** | `struct MotifDirective { motif: MotifId, scope: MotifScope, effects: Vec<MotifEffect> }`. Effects: `AddRoomTag`, `PreferArchetype`, `RequireFeature`, `AddTerrainScatter`, `AddEntityPressure`. Vocabularies can emit directives alongside labels/tags. **Application is phased explicitly:** intent-phase effects (PreferArchetype) run before spatial planning; spatial-phase effects (AddRoomTag) run before geometry; composition-phase effects (RequireFeature) run before feature planning; entity-phase effects (AddEntityPressure) run before entity planning. Not separate structs yet, but phase of application is a field on each effect. |
| 4.7 | **New Scenario: Rat-Infested Port Cellar** | Extends the tavern vocabulary with a `"vermin"` motif. The motif adds `"infested"` tags to storage rooms, requires `"rat_nest"` features near food, adds rat entity pressure. Proves motif directives + atmosphere profiles produce coherent themed rooms without per-scenario code. |

**Design decisions:**
- InteriorPlan is computed *after* GeometryPlan (needs concrete rect/footprint) but *before* FeaturePlan (features need zones). It sits alongside TileMap, not after it — the pipeline is a DAG from GeometryPlan onward.
- Templates are matched by `(archetype, theme_tags)` — a room can fall back to rule-based placement if no template matches.
- Irregular footprints are a geometry-layer concern; scatter + features work on any footprint shape via `Footprint::cells()` iterator.
- MotifDirectives do NOT create new architectural seams between stages. They inject tags/requirements into existing data structures that downstream stages already read. But application is *phased*: each effect declares which pipeline stage it targets (intent, spatial, composition, feature, entity). This prevents spooky action at a distance.
- Atmosphere profiles replace the proliferation of one-off tag→feature rules. A profile bundles scatter + features + entities into a coherent palette. The planner *samples from* the palette rather than deterministically firing every matching rule.

**Caution:** Tags must remain *soft intent*, not secret bytecode. When rules start needing negation, priority ordering everywhere, or "unless" clauses, the concept must be promoted into typed Rust. Tags are good for signaling; tags are bad as undocumented control flow.

**Exit criterion:** Cave rooms have visibly irregular shapes. At least 2 room templates produce spatial layouts (not random scatter). A motif demonstrably alters room content across a scenario without per-scenario code. Reserved paths are validated: templates and required features cannot block door-to-door traversal. Tags are classified — structural vs. atmospheric intent is visible in data, not implicit.

---

### Phase 5: Pacing, Composition & Scale

**Goal:** Generate larger maps with narrative pacing — buildup, climax, release. Patterns compose into richer structures.

| # | Task | Description |
|---|---|---|
| 5.1 | **Pattern Composition** | A slot in one pattern can expand into a sub-pattern. `MapScale` controls expansion budget: Tiny=1 pattern, Small=1+1 expansion, Medium=1+2, Large=recursive. |
| 5.2 | **Pacing Curve** | Assign tension scores to rooms by graph distance from Entry. Tag rooms with `tension: low/medium/high/climax`. |
| 5.3 | **Tension-aware entity density** | Entity rules gain `match_tension: Option<TensionLevel>`. High-tension rooms get more/harder entities. Low-tension rooms stay sparse. |
| 5.4 | **Rest points** | If critical path length > N, insert a `Reward`-role room mid-path (safe, contains supplies). Pattern-level insertion. |
| 5.5 | **Pacing validator** | Warns if the tension curve is flat (boring), immediately maxed (unfair), or has no climax before the goal. |
| 5.6 | **MotifEffect: structural** | Extend motif effects to alter graph structure: `BlockOrAlterLink` (collapse blocks a corridor, forcing detour), `AddSecondaryConnection` (add a shortcut or secret passage). Motifs now reach into MapIntent → SpatialPlan transitions. |
| 5.7 | **New Scenario: Abandoned Mine** | Medium-scale (10–15 rooms). Main shaft (linear descent) + branching galleries (hub-and-spoke at depth). Tests pattern composition + pacing over a longer path. |

**Exit criterion:** Medium-scale maps have measurable pacing curves visible in `--trace` output. Validator catches degenerate structures. A motif can structurally alter the graph (blocked passage, forced detour).

---

### Phase 6: Algorithmic Depth (scenario-driven)

**Goal:** Improve generation quality at individual layers, motivated by concrete scenario needs.

| Scenario Need | Layer | Approach |
|---|---|---|
| Corridors are monotonous | Routing | A* with cost maps; wider corridors for Hall connections |
| No encounter design | Entities | Encounter budgets per room (difficulty ≤ tension × budget) |
| Can't detect boring maps | Validation | Branching factor check, dead-end density, reachability analysis |
| Tower needs vertical movement | Geometry | Multi-level support: `PlacedSpace` gains `z_level`, stairwell routing |
| Feature groups feel isolated | Features | Template composition: sub-templates that reference each other (altar *with* candles *with* offering bowl) |
| Motifs need world-level coherence | Pipeline | Motif propagation across rooms: a flooded canal motif marks rooms along a line, not just individual spaces |

**Priority is driven by which new scenario needs it.** No algorithm work happens without a scenario that exercises it.

---

### Scenario Roadmap

| Scenario | Key Tests | Unlocked by |
|---|---|---|
| Noble Crypt | lock-and-key, undead theme | Foundation ✅ |
| Tavern Cellar | branching, urban theme, secrets | Foundation ✅ |
| Natural Cave | exploration, scatter terrain, irregular shapes | Phase 3 ✅ (shapes: Phase 4) |
| Rat-Infested Port Cellar | motif directives, entity pressure, infested rooms | Phase 4 |
| Wizard's Tower | vertical traversal, arcane theme, multi-level | Phase 5 |
| Abandoned Mine | pattern composition, medium scale, pacing | Phase 5 |
| Thieves' Guild | complex hub-and-spoke, traps, structural motifs | Phase 5 |
| Dragon's Lair | large scale, encounter budgets, boss room | Phase 6 |

---

## Architecture Reference

### Pipeline Data Flow

```
SituationContext            (narrative tags + bindings)
    │
    ▼
MapIntent                   (structural graph, scale, constraints, motifs)
    │
    ▼
SpatialPlan                 (abstract spaces + links + size hints)
    │
    ▼
GeometryPlan                (placed rects/footprints + corridor polylines)
    ├───→ TileMap + Scatter   (2D grid of TileIds, terrain by room tags)
    └───→ [InteriorPlan]     (Phase 4: zones, reserved paths, feature requests)
                  │
    TileMap ──────┘
                  ▼
         FeaturePlan        (placed furniture/traps/decorations)
                  │
                  ▼
         EntityPlan         (placed monsters/NPCs with behavior)
                  │
                  ▼
         Validation → retry loop if errors
                  │
                  ▼
         ASCII / Export
```

Note: `InteriorPlan` (Phase 4) depends on `GeometryPlan` (needs concrete room shapes) and informs `FeaturePlan` (features respect zones + reserved paths). `TileMap` also flows into `FeaturePlan` for tile-aware placement. The stages are not strictly linear — it’s a DAG from `GeometryPlan` onward.

### Key Types (structural — stay as enums)

```rust
enum NodeRole    { Entry, Hub, Gate, Goal, Reward, Branch, Transition }
enum EdgeRole    { Traversal, OptionalTraversal, RestrictedTraversal, SecretTraversal, VerticalTraversal }
enum Severity    { Error, Warning, Info }
```

### Key Types (content — becoming data-driven)

```rust
// Phase 3 (done): Tile enum collapsed to TileId + TileRegistry
// TileId(u16) + TileRegistry { name, walkable, opaque, ascii_char, tags }

// Phase 4 (planned): FeatureKind enum → feature_type: String + FeatureRegistry
// FeatureCategory enum stays for behavior dispatch:
enum FeatureCategory { Furniture, Container, Trap, Decoration, Interactable }
// FeatureRegistry maps type name → { category, ascii_char, blocking, tags }

// Already data-driven (string newtypes):
struct Tag(String)
struct EntityArchetypeId(String)
struct MotifId(String)
```

### Data Flow Rules

- **Down is default.** Each layer's output is the next layer's input.
- **Cross-references are explicit.** FeaturePlan receives `SpatialPlan + GeometryPlan + TileMap`.
- **Validators trigger reruns, not mutations.** On failure, constraints relax and the layer re-executes.

---

## Design Decisions

### Why types vs. data?

Types (enums) are for things that change the *control flow* — `NodeRole::Entry` triggers the "skip entities" logic, `EdgeRole::RestrictedTraversal` triggers locked-door placement. These need exhaustive matching.

Data (JSON) is for things that change the *content* — which label a room gets, which features spawn, what entities appear. These should be open-ended without recompilation.

### Why patterns + vocabularies (not LLM prompts)?

Deterministic, testable, seed-reproducible. An LLM could *produce* patterns and vocabularies as content authoring, but the runtime generation must be fully deterministic given a seed.

### Why separate tiles from features?

Tiles define *terrain* (affects pathfinding, line-of-sight). Features define *objects on terrain* (can block movement but are semantically different). A chest is not a tile — it's a feature placed on a floor tile.

### Why property-based testing matters here?

The system is becoming highly compositional: random patterns × random vocabularies × random seeds × random rules = exponential combinations. You can’t hand-write enough integration tests. Proptest generates arbitrary valid inputs and asserts invariants (connectivity, no overlaps, safe entry rooms) hold for *all* of them.

### Why room composition/zones (not just scatter rules)?

Scatter rules place individual objects. But a room’s identity is *spatial*: the altar is at the center, framed by candles, with a clear processional path to the door. That requires a layer between geometry and features that declares zones and reserves paths. Without it, FeaturePlan must infer too much from raw tiles, and features feel randomly strewn.

### Why motifs inject into existing data, not a new pipeline stage?

Motifs are cross-cutting: they add tags, prefer archetypes, require features, alter scatter. If each effect were a new pipeline stage, the architecture would fracture. Instead, motifs produce *directives* that mutate existing data structures (add a tag to a SpaceSpec, append a rule to feature rules) before the downstream stage runs. The stages stay clean; motifs are a pre-processing enrichment pass.

### When do tags become too powerful?

Tags are soft intent — they signal, they don’t command. Danger signs: rules needing negation (`not: "damp"`), priority ordering everywhere, "unless" clauses, tags encoding procedural logic. When that happens, promote the concept into typed Rust (a new enum variant, a new struct field, a trait method). JSON should never become an accidentally-invented scripting language.

The specific risk: if tags simultaneously serve as descriptive vocabulary, control-flow triggers, feature selectors, terrain selectors, *and* entity selectors, they become secret bytecode. Phase 4 addresses this by classifying tags into typed fields:

```rust
pub struct SpaceSpec {
    pub structural_tags: Vec<Tag>,  // main_goal, locked — hard causal triggers
    pub atmosphere: Vec<Tag>,       // damp, echoing — soft flavor signals
    pub motifs: Vec<MotifId>,       // rat_infestation — cross-cutting directives
    pub tags: Vec<Tag>,             // general/unclassified (backward compat)
}
```

### Three tiers of asset rules

Not all rules should work the same way:

**1. Structural rules** — explicit, typed, engine behavior:
- `RestrictedTraversal` → locked door placement
- Entry rooms skip hostile entities
- Pattern votes from situation tags
- Tile walkability/opacity

These are causal and must remain visible in code.

**2. Content rules** — explicit data, direct matching:
- `Vault + main_goal` → Sarcophagus required
- `locked Gate` → guardian near door
- Tile scatter: `flooded` → Water at 15%

These are good as direct JSON rules with clear match criteria.

**3. Atmospheric influences** — bundled, weighted, profile-based:
- `damp + underground` → { water, moss, rats } weighted palette
- `noble + crypt` → { sarcophagus, altar, candles } palette
- `port + cellar` → { barrels, crates, rats } palette

These should be atmosphere profiles (Phase 4.2), not hundreds of one-off rules. Rooms accumulate influences from matching profiles and the planner samples from the merged palette. Less spreadsheet wiring, more coherent taste.

---

## Success Criteria

The system is "done" when:

1. `Pipeline::run(situation)` produces a valid map for *any* well-formed situation context — no per-scenario code needed.
2. Adding a new scenario = authoring data files (vocabulary JSON + pattern selection tags). Zero Rust changes.
3. The same situation + different seeds → different maps that all pass validation and feel thematically coherent.
4. Property-based tests confirm invariants hold across thousands of random combinations.
5. At least 4 distinct scenarios demonstrate meaningfully different structures and themes.
6. `cargo run` always works, `cargo test` is always green.
