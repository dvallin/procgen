# PLAN.md — Roadmap

## Vision

A story-driven procedural generator where narrative signals steer every decision. Given a `SituationContext` ("sealed crypt, noble family, locked vault"), the system infers structure, materializes themed rooms, places narratively appropriate features and entities, and produces a map that feels *designed* — with pacing, tension, and coherence — without any hand-authored graph.

---

## Where We Are

The pipeline skeleton is complete and validated. Four demo scenarios (crypt, tavern, cave, cellar) flow through all layers and produce ASCII output. 392 tests (property-based + integration + regression + doctests) confirm layer invariants hold for arbitrary inputs. Seeded RNG ensures reproducibility. **Phases 1–4 are complete.**

```
SituationContext → IntentBuilder → MapIntent → SpatialPlan → GeometryPlan → TileMap (+scatter) → InteriorPlan → FeaturePlan → EntityPlan → ASCII
```

**Completed capabilities:**
- Full pipeline runner with retry semantics and seeded RNG
- Data-driven intent generation: pattern selection → vocabulary lookup → slot filling → constraint inference
- 5 narrative patterns (JSON), weighted voting + explicit override via `bindings["pattern"]`
- 3 theme vocabularies + overlay inheritance (`base` field for DRY vocabulary composition)
- Tile registry (`TileId(u16)` + `TileRegistry`) with JSON-defined tiles and scatter rules
- Feature registry (`FeatureType(String)` + `FeatureRegistry`) with JSON-defined feature types
- Atmosphere profiles: weighted scatter/feature/entity palettes, tag-matched, sampled per room
- Room zones (InteriorPlan) + interior templates (6 starters, zone→feature directives)
- Irregular room shapes (CaveIrregularizer for cave/natural rooms)
- Entity placement with patrol zones, safe entry rooms, density caps
- Validators at geometry, feature, and entity levels
- Additive rule injection: `PipelineConfig.additional_{feature_rules,entity_rules,atmospheres}`
- PipelineResult annotations: room labels, roles, positions, named entities
- Situation directives: `PinEntity` guarantees named entities in target rooms
- Property-based stress tests with random spatial plans
- Four demo scenarios: crypt, tavern, cave, cellar (with Skrag the Rat King pinned into Goal)

**What's next: Phase 5 — Pacing, Composition & Scale**

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

### Phase 4: Room Character, Atmosphere & World-Engine Flexibility ✅

**Goal:** Rooms gain spatial identity through constrained templates and zones. Features become data-driven like tiles. The engine becomes flexible enough for a world engine to compose themes, control structure, inject quest content, and read back semantic results — all without per-scenario code.

| # | Task | Description |
|---|---|---|
| 4.1 | **Feature Registry & FeatureCategory enum** | ✅ | Mirror the tile registry pattern for features. Replace `FeatureKind` enum variants with a `feature_type: String` identifier + `FeatureRegistry` mapping type name → `FeatureProperties { category, ascii_char, blocking, tags }`. `FeatureCategory` enum (Furniture, Container, Trap, Decoration, Interactable) stays for gameplay behavior dispatch. Feature rules reference type strings ("altar", "chest") instead of enum variants. New feature types added via JSON without code changes. |
| 4.2 | **Tag classification & atmosphere profiles** | ✅ | Split the flat `tags: Vec<Tag>` into typed fields on SpaceSpec: `structural_tags` (main_goal, locked — hard triggers), `atmosphere` (damp, echoing — soft flavor), `motifs` (rat_infestation — cross-cutting directives). Introduce **atmosphere profiles** (`assets/rules/atmospheres.json`): bundles of weighted scatter/feature/entity influences keyed by atmosphere combination. Rooms accumulate influences from matching profiles; the planner samples from the resulting palette. Replaces brittle one-off rules like `{ match_tag: "vast", feature: "stalagmite" }` with coherent taste bundles. |
| 4.3 | **Room zones & InteriorPlan** | ✅ | `InteriorPlan { space_id, rect, zones, doors, reserved_paths, path_intents }`. Zones partition a room's interior into `ZoneKind::{Center, WallBand, Corner, DoorPath, Open}`. Reserved paths (door-to-door BFS shortest paths) are pre-computed once and enforced by both scatter (non-walkable tiles never placed on reserved cells) and the feature planner (blocking features excluded from reserved cells via `pick_candidate`). `doors_reachable` BFS retained as safety-net validation. Computed after rasterization+scatter, consumed by FeaturePlan. |
| 4.4 | **Template interiors (constraint-based)** | ✅ | Not fixed prefabs — declarative templates with spatial constraints. E.g. `crypt_vault: { center: altar, wall-band: candles, clear: door-path }`. `tavern_storage: { wall-band: shelves, corners: barrels, center: clear }`. Templates produce an `InteriorPlan`; the feature planner respects it. |
| 4.5 | **Irregular room shapes** | ✅ | Introduce `ShapeRefinement` trait as a composable per-room shape transform. `CaveIrregularizer` is the first non-trivial implementation: starts with a rect, carves corners, roughens edges, preserves connectivity. Rooms with `LocationKind::Cave` or tag `"natural"` use it. Fixes the "office building under moss" look. Architecture separates layout (WHERE rooms go) from shape (WHAT each room looks like) — future algorithms (BSP, L-systems, cellular automata) can replace layout independently or compose with existing shape refinements. |
| 4.6 | **MotifDirective** | ⏭️ Scrapped | Analysis showed that vocabulary entries (which put tags on rooms per-role) + atmosphere profiles (which react to tags to produce placements) already cover the same ground. The `Composition`/`Entity`-phase effects (`RequireFeature`, `AddTerrainScatter`, `AddEntityPressure`) duplicate atmosphere profiles with less expressiveness (no placement strategy, no weights). The `Intent`/`Spatial`-phase effects (`AddRoomTag`, `PreferArchetype`) can be achieved by putting the right tags/archetypes directly in vocabulary entries. The one genuinely novel capability (cross-cutting `MatchTag` scope — reacting to tags you didn't put there) has no concrete scenario requiring it yet. Reintroduce if a future scenario can't be expressed without it. |
| 4.7 | **New Scenario: Rat-Infested Port Cellar** | ✅ | New `vermin_cellar` vocabulary with `"infested"` tags on storage/food/hub rooms. New `vermin_infested` atmosphere profile (match tags: `infested`, `nest`) produces rat_nest features, barrel furniture, rat + giant_rat entities. `src/demo/cellar.rs` + CLI wiring (`cargo run -- cellar`). **Proves the design thesis:** vocabulary entries put the right tags on the right rooms → atmosphere profiles react to those tags → coherent themed content emerges. Zero per-scenario generation code needed beyond the 12-line situation factory. 4 integration tests validate connectivity, rat/nest presence, and atmosphere activation. |
| 4.8 | **Explicit pattern override** | ✅ | Allow `bindings["pattern"]` to bypass tag-based voting and force a specific narrative pattern. Fallback: when absent, the current weighted-vote selector runs as before. **Demo change:** new integration test constructs a `vermin_cellar` situation with `pattern = "linear_descent"` and verifies the resulting graph is a linear chain (not hub-and-spoke). Proves a world engine can control dungeon *shape* independently of dungeon *theme*. |
| 4.9 | **Vocabulary overlays** | ✅ | Replace monolithic vocabulary copies with composable layers. A `ThemeVocabulary` gains an optional `base: Option<String>` field; if present, it inherits all roles from the base and the overlay only specifies additions/overrides per role. **Demo change:** `vermin_cellar` vocabulary shrinks to `{ base: "urban_underground", roles: [only the overridden entries] }`. Existing `cargo run -- cellar` output is identical but the vocabulary file is smaller and DRY. New integration test: construct a novel overlay at runtime (inject via `PipelineConfig`) without pre-authoring a full vocabulary JSON — proves the world engine can compose themes dynamically. |
| 4.10 | **Additive rule injection** | ✅ | `PipelineConfig` gains `additional_feature_rules`, `additional_entity_rules`, `additional_atmospheres` that are *merged* with defaults (not replacing them). **Demo change:** new integration test runs the crypt scenario with an injected `"cursed"` atmosphere profile (adds skeleton entities to all rooms with `"sacred"` tag). Verifies skeletons appear in the chapel without touching any JSON files. Proves a quest system can layer temporary effects onto the generator. |
| 4.11 | **PipelineResult annotations** | ✅ | `PipelineResult` gains `annotations: Vec<RoomAnnotation>` carrying each room's label, role, structural tags, bounding rect, and door positions. Entry/exit rooms are flagged. **Demo change:** `cargo run -- --annotate` prints a legend below the ASCII map listing room labels + roles (e.g. `[A] Main Cellar (Hub) [B] Rat King's Den (Goal)`). Letters are overlaid on the map at room centers. Integration test verifies annotations are present and consistent with geometry. Proves the world engine can correlate map output with narrative graph nodes. |
| 4.12 | **Situation directives (entity pinning)** | ✅ | `SituationContext` gains `directives: Vec<SituationDirective>`. First directive: `PinEntity { archetype, name, target_role }` — guarantees a named entity appears in a room matching the target role. Entity planner checks directives before rule-based placement; pinned entities bypass normal budget/weight logic. **Demo change:** cellar situation adds `PinEntity { archetype: "giant_rat", name: "Skrag the Rat King", target_role: Goal }`. `PipelineResult.annotations` includes named entity positions. Integration test verifies Skrag appears in the Goal room specifically. Proves a quest system can place quest targets deterministically. |

**Design decisions:**
- InteriorPlan is computed *after* GeometryPlan (needs concrete rect/footprint) but *before* FeaturePlan (features need zones). It sits alongside TileMap, not after it — the pipeline is a DAG from GeometryPlan onward.
- Templates are matched by `(archetype, theme_tags)` — a room can fall back to rule-based placement if no template matches.
- Irregular footprints are a geometry-layer concern; scatter + features work on any footprint shape via `Footprint::cells()` iterator.
- **Vocabulary entries are the primary theming lever.** Tags on entries drive atmosphere profiles downstream. This is simpler and more transparent than a separate MotifDirective system — you can read a vocabulary entry and see exactly what tags it will produce, and then read the matching atmosphere profile to see what content that triggers.
- Atmosphere profiles replace the proliferation of one-off tag→feature rules. A profile bundles scatter + features + entities into a coherent palette. The planner *samples from* the palette rather than deterministically firing every matching rule.
- MotifDirectives were prototyped and scrapped: they duplicated atmosphere functionality with less expressiveness. The cross-cutting `MatchTag` capability (reacting to tags you didn't author) is the only novel affordance — reintroduce when a scenario needs it.

**Geometry architecture (introduced in 4.5):**

`ColumnGeometryPlanner` conflates three concerns that should be independently replaceable:
1. **Layout** — where rooms sit relative to each other (BFS columns currently; BSP, force-directed, L-system in future).
2. **Shape** — what each room's footprint looks like (`ShapeRefinement` trait; rect, cave irregular, cellular automata, L-shaped in future).
3. **Routing** — how corridors connect rooms (already abstracted via `CorridorRouter` trait).

Shape is extracted in 4.5 as the `ShapeRefinement` trait. Layout stays inlined in `ColumnGeometryPlanner` for now — extract when a second layout algorithm arrives. The composition:
```
GeometryPlanner (trait — public pipeline contract)
├── ColumnGeometryPlanner (composes):
│   ├── Layout: BFS columns (internal, not yet a trait)
│   ├── ShapeRefinement: per-room footprint transform (trait)
│   │   ├── RectShape (no-op, default)
│   │   └── CaveIrregularizer (4.5)
│   └── CorridorRouter: Z-shape (existing trait)
├── (future) BSPGeometryPlanner
├── (future) CellularAutomataPlanner
└── (future) HierarchicalGeometryPlanner
```

**4.5 subtasks:**
| # | Subtask | Description |
|---|---------|-------------|
| 4.5.1 | `ShapeRefinement` trait + `CaveIrregularizer` | ✅ | New `src/geometry/shape.rs`. Trait: `fn refine(rect, space, rng) → Footprint`. `RectShape` (no-op) + `CaveIrregularizer` (carve corners, roughen edges, connectivity BFS, min-cell-count fallback). |
| 4.5.2 | Thread RNG into geometry planning | ✅ | Extend `GeometryPlanner::plan` signature to accept `&mut dyn RngCore`. Update all call sites (pipeline, tests). Mechanical but necessary for deterministic irregularization. |
| 4.5.3 | Wire shape refinement into `ColumnGeometryPlanner` | ✅ | After BFS placement, iterate `PlacedSpace`s. Select `ShapeRefinement` per room: `CaveIrregularizer` if `LocationKind::Cave` or tag `"natural"` on rooms ≥ 7×7, else `RectShape`. Propagate `LocationKind` via `SpatialPlan`. |
| 4.5.4 | Downstream compatibility | ✅ | Verify/fix zone classification, scatter, feature placement, reserved paths for `Footprint::Cells`. Zones iterate actual floor cells, not rect interior. |
| 4.5.5 | Tests | ✅ | Unit tests (connectivity, subset-of-rect, min-threshold, determinism). Proptest (any rect ≥ 5×5 produces connected result). Integration: `cargo run -- cave` shows non-rectangular rooms. Existing tests unaffected. |
| 4.5.6 | Move shape refinement after routing (cleanup) | ⏭️ Skipped | The protected cross + `connect_doors_to_rooms` band-aid is cheap, correct, and keeps shape/routing decoupled. Revisit only if a future shape strategy breaks the "carve inward until floor" heuristic. |

**Caution:** Tags must remain *soft intent*, not secret bytecode. When rules start needing negation, priority ordering everywhere, or "unless" clauses, the concept must be promoted into typed Rust. Tags are good for signaling; tags are bad as undocumented control flow.

**Exit criterion:** Cave rooms have visibly irregular shapes. At least 2 room templates produce spatial layouts (not random scatter). Vocabulary entries + atmosphere profiles demonstrably produce coherent themed rooms across a scenario without per-scenario code (proven by the `cellar` scenario). Reserved paths are validated: templates and required features cannot block door-to-door traversal. Tags are classified — structural vs. atmospheric intent is visible in data, not implicit. A world engine can: (a) force a specific pattern independent of theme, (b) compose a vocabulary from base + overlay without monolithic copies, (c) inject additional rules at runtime, (d) read back room roles/labels/positions from the result, (e) pin a named quest entity into a specific role's room.

---

### Phase 5: Pacing, Composition & Scale

**Goal:** Generate larger maps with narrative pacing — buildup, climax, release. Patterns compose into richer structures. The geometry layer scales gracefully to 10–15 rooms.

| # | Task | Description |
|---|---|---|
| 5.1 | **Force-Directed Geometry Planner** | Replace the BFS column layout (`ColumnGeometryPlanner`, renamed from `ColumnGeometryPlanner`) with a force-directed alternative (`ForceDirectedGeometryPlanner`). Nodes attract along graph edges, repel non-adjacent nodes, avoid overlap. Produces more natural spatial relationships for larger room counts. Both implementations stay available behind the `GeometryPlanner` trait; `PipelineConfig` selects which one to use. Rename `ColumnGeometryPlanner` → `ColumnGeometryPlanner` to clarify what it actually does. |
| 5.2 | **Pattern Composition** | A slot in one pattern can be marked `expandable: true`. When the filler encounters an expandable slot (and budget permits), it runs `select_pattern` on a filtered pattern library to dynamically choose a sub-pattern — reusing the same situation-tag voting mechanism as top-level selection. The sub-pattern's graph is grafted into the parent (Entry node replaces the slot, internal edges/nodes merged with key namespacing). `MapScale` controls expansion budget: Tiny=0, Small=1, Medium=2, Large=3+. Composed patterns produce 8–15 node graphs. No hard-coded pattern IDs — new patterns become auto-eligible via their votes. |
| 5.3 | **Entity Archetype Registry** | `assets/rules/entity_archetypes.json` defines entity archetypes: `{ id, display_char, difficulty_tier, behavior_tags, default_patrol }`. Tiers: `minion`, `standard`, `elite`, `boss`. The entity planner references archetypes by ID; the registry resolves display char and tier. Provides the variety needed for difficulty scaling. Existing `entities.json` rules reference these IDs (backward-compatible). |
| 5.4 | **Pacing Curve** | Assign tension scores to rooms by graph distance from Entry along the critical path. Tag rooms with `tension: low/medium/high/climax`. Tension is a computed property on `RoomAnnotation`, not a tag — it's structural, not content. Critical path = shortest Entry→Goal path in the structural graph. |
| 5.5 | **Tension-aware entity density** | Entity rules gain `match_tension: Option<TensionLevel>`. High-tension rooms get more/harder entities (higher tier). Low-tension rooms stay sparse (minions only). Atmosphere profiles can also reference tension for scatter intensity (more rubble near climax). |
| 5.6 | **Rest points** | If critical path length > N, insert a `Reward`-role room mid-path (safe, contains supplies). Pattern-level insertion during composition. Rest points get `tension: low` regardless of depth. |
| 5.7 | **Pacing validator** | Warns if the tension curve is flat (no buildup), immediately maxed (unfair), or has no climax before the goal. Reports via `PipelineResult.annotations` so the world engine can react. |
| 5.8 | **New Scenario: Abandoned Mine** | Medium-scale (10–15 rooms). Main shaft (linear descent) + branching galleries (hub-and-spoke at depth). Tests pattern composition + force-directed layout + pacing over a longer path. New vocabulary `abandoned_mine`. |

**Design notes:**
- 5.1 comes first because pattern composition (5.2) will produce 10–15 room graphs that the BFS column layout handles poorly. Better to have a solid geometry foundation before scaling up room count.
- Entity archetype registry (5.3) provides the variety needed for tension-aware density (5.5) to be meaningful — without it, "harder entities" has nowhere to go.
- Structural motif effects (graph mutations like `BlockOrAlterLink`, `AddSecondaryConnection`) are **deferred** — the same reasoning as Phase 4.6 applies: pattern selection + vocabulary already control structure. Reintroduce only if the Abandoned Mine scenario or a future scenario genuinely can't express its structure via pattern composition alone.

**Exit criterion:** Medium-scale maps (10–15 rooms) have natural spatial layouts via force-directed placement, measurable pacing curves visible in `--trace` output, and tension-appropriate entity populations. Pacing validator catches degenerate structures.

**5.2 subtasks:**
| # | Subtask | Description |
|---|---------|-------------|
| 5.2.1 | Data model: `expandable` on `PatternSlot` | Add `expandable: bool` (serde default false) to `PatternSlot`. Add optional `expansion_votes: Vec<ExpansionVote>` to `NarrativePattern` — each entry is `{ parent_role: NodeRole, weight: i32 }` giving the pattern affinity for expanding slots of that role. |
| 5.2.2 | `PipelineConfig.expansion_budget` | Add `expansion_budget: Option<u32>` to `PipelineConfig`. When `None`, derive from `MapScale` (Tiny=0, Small=1, Medium=2, Large=3, Huge=4). Allow override from `SituationContext.bindings["expansion_budget"]`. |
| 5.2.3 | Composition logic in filler | Extend `fill_pattern` to accept `&[NarrativePattern]` (full library) + `expansion_budget: u32`. For expandable slots: (a) budget>0 check, (b) RNG inclusion roll, (c) filter library (exclude self, exclude patterns with required slots > remaining budget), (d) run `select_pattern` on filtered set (with expansion_votes as bonus), (e) recursively fill sub-pattern (decrement budget), (f) graft sub-graph into parent. |
| 5.2.4 | Graph grafting helper | `graft_subgraph(parent_nodes, parent_edges, slot_key, sub_graph, next_id)` — merges sub-pattern nodes (with key namespacing: `slot_key.sub_key`), rewires parent edges pointing at slot to sub-pattern's Entry node, re-numbers node IDs for uniqueness. |
| 5.2.5 | Wire into `GenericIntentBuilder` | Pass pattern library + budget to the filler. Budget derived from config or scale. |
| 5.2.6 | Annotate `narrative.json` | Mark selected slots as `expandable: true` in existing patterns: `branching_exploration.branch_a`, `branching_exploration.branch_b`, `hub_and_spoke.spoke_1`, `lock_and_key.key_area`. Optionally add `expansion_votes` to patterns (e.g., `linear_descent` gets `[{ parent_role: "Branch", weight: 2 }]`). |
| 5.2.7 | Tests | Unit: expandable deserializes, budget=0 prevents expansion, budget=1 expands one slot, composed graph edges valid, key namespacing correct. Proptest: any expandable pattern + vocabulary produces connected graph with valid edges. Integration: medium-scale situation produces 8–15 room graph. |

**5.2 design decisions:**
- **Voting over hard-coded IDs.** Sub-pattern selection reuses `select_pattern` — situation tags guide composition contextually. Adding a new pattern to the library automatically makes it eligible for expansion without editing existing patterns.
- **`expansion_votes` is optional, not required.** Without it, pure situation-tag voting works. The field exists for fine-tuning ("linear_descent prefers to expand Branch slots") without creating pattern-to-pattern coupling.
- **Self-exclusion prevents infinite recursion.** The current pattern (and its ancestors in the recursion stack) are excluded from sub-selection. Combined with budget, this guarantees termination.
- **Key namespacing preserves traceability.** A node `branch_a.hub.gate` tells you exactly where it came from in the composition tree. Annotations and debugging benefit from this.
- **Downstream is transparent.** The composed `ScenarioGraph` looks like any other graph to spatial/geometry/tile layers. No changes needed downstream — force-directed layout (5.1) handles the larger node counts.

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
| Rat-Infested Port Cellar | motif directives, entity pressure, infested rooms | Phase 4 ✅ |
| Abandoned Mine | pattern composition, medium scale, pacing, force-directed layout | Phase 5 |
| Wizard's Tower | vertical traversal, arcane theme, multi-level | Phase 6 |
| Thieves' Guild | complex hub-and-spoke, traps, encounter budgets | Phase 6 |
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

// Phase 4 (done): FeatureKind enum → feature_type: String + FeatureRegistry
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
