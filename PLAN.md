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

| # | Task | Status | Description |
|---|---|---|---|
| 5.1 | **Force-Directed Geometry Planner** | ✅ | Replace the BFS column layout (`ColumnGeometryPlanner`, renamed from `ColumnGeometryPlanner`) with a force-directed alternative (`ForceDirectedGeometryPlanner`). Nodes attract along graph edges, repel non-adjacent nodes, avoid overlap. Produces more natural spatial relationships for larger room counts. Both implementations stay available behind the `GeometryPlanner` trait; `PipelineConfig` selects which one to use. Rename `ColumnGeometryPlanner` → `ColumnGeometryPlanner` to clarify what it actually does. |
| 5.2 | **Pattern Composition** | ✅ | `PatternSlot` gains `expandable: bool`; `NarrativePattern` gains `expansion_votes: Vec<ExpansionVote>` for role-affinity scoring. When the filler encounters an expandable slot (budget permitting), it runs `select_pattern` on a filtered pattern library — reusing the same voting mechanism as top-level selection, plus expansion vote bonuses. Sub-pattern graph is grafted into the parent (Entry node replaces the slot, internal edges/nodes merged with key namespacing e.g. `branch_a.hub`). Budget controlled via `PipelineConfig.expansion_budget` or `SituationContext.bindings["expansion_budget"]`; default 0 (backward-compatible). Cave demo uses budget=1, producing 8–12 room sprawling maps. New `src/intent/compose.rs` module with `fill_pattern_composed`, `CompositionContext`, `graft_subgraph`. No hard-coded pattern IDs — new patterns become auto-eligible via their votes. |
| 5.3 | **Entity Archetype Registry** | ✅ | `EntityCategory` enum (`Creature`, `NPC`, `Hazard`, `Boss`) + `EntityProperties` struct + `EntityRegistry` (HashMap-based, mirrors `FeatureRegistry`). 11 built-in entity types defined in `assets/rules/entity_types.json` with embedded fallback. Hardcoded `entity_char()` substring matching replaced by registry lookups. ASCII render functions accept `&EntityRegistry`. `PipelineConfig.entity_registry` for overrides. |
| 5.4 | **Pacing Curve** | ✅ | `TensionLevel` enum (`Low`, `Medium`, `High`, `Climax`) assigned per room by BFS distance from Entry normalized against critical path (shortest Entry→Goal). `RoomAnnotation.tension` computed automatically. Goal rooms always `Climax`, Entry/Reward always `Low`. Visible in `--annotate` legend and `--trace debug` output. New `src/tension.rs` module with `compute_tension()` + 7 unit tests. |
| 5.5 | **Tension-aware entity density** | ✅ | `TensionLevel` gains `Ord` + `density_multiplier()` (Low=0.5×, Medium=1.0×, High=1.5×, Climax=2.0×). `EntityRule.tension_min: Option<TensionLevel>` gates rules by minimum room tension. Entity planner scales density cap by multiplier and filters rules by tension. 4 structural entity rules gated at Medium/High; quest-critical rules bypass tension. |
| 5.6 | **ANSI Colored Rendering** | ✅ | `src/tile/color.rs` module adds colored ASCII output. Entities by danger, features by importance, tiles by type. CLI `--color` flag (`auto`/`on`/`off`). Zero dependencies (raw ANSI codes). |
| 5.7 | **Rest Points & Seed Reporting** | ✅ | Role-based tension caps: Transition capped at Medium, Hub capped at High. Sub-pattern Entry nodes converted to Transition during grafting. **Rest point insertion** (`rest_points.rs`): post-composition, if critical path > 5 edges and no natural Low room mid-path, a Transition node with `rest_point` tag is inserted by splitting a Traversal edge near the midpoint. Tension forces `rest_point`-tagged rooms to Low. `PipelineResult.seed: u64` stores actual seed. CLI prints `(seed: N)`. 6 rest_points tests + 4 tension tests. |
| 5.8 | **Pacing validator** | ✅ | `PacingValidator` checks tension curve for degenerate patterns: flat curve (all same), no climax (no peak), immediate climax (too abrupt), no rest in long path. Produces Warnings/Infos only (never Errors). `PipelineResult.pacing_warnings: Vec<String>` carries issues. CLI prints ⚠ when present. 8 unit tests. |
| 5.9 | **New Scenario: Abandoned Mine** | | Medium-scale (10–15 rooms). Main shaft (linear descent) + branching galleries (hub-and-spoke at depth). Tests pattern composition + force-directed layout + pacing over a longer path. New vocabulary `abandoned_mine`. |

**Design notes:**
- 5.1 comes first because pattern composition (5.2) will produce 10–15 room graphs that the BFS column layout handles poorly. Better to have a solid geometry foundation before scaling up room count.
- Entity archetype registry (5.3) provides the variety needed for tension-aware density (5.5) to be meaningful — without it, "harder entities" has nowhere to go.
- Structural motif effects (graph mutations like `BlockOrAlterLink`, `AddSecondaryConnection`) are **deferred** — the same reasoning as Phase 4.6 applies: pattern selection + vocabulary already control structure. Reintroduce only if the Abandoned Mine scenario or a future scenario genuinely can't express its structure via pattern composition alone.

**Exit criterion:** Medium-scale maps (10–15 rooms) have natural spatial layouts via force-directed placement, measurable pacing curves visible in `--trace` output, and tension-appropriate entity populations. Pacing validator catches degenerate structures.

**5.2 subtasks:**
| # | Subtask | Status | Description |
|---|---------|--------|-------------|
| 5.2.1 | Data model: `expandable` on `PatternSlot` | ✅ | Add `expandable: bool` (serde default false) to `PatternSlot`. Add optional `expansion_votes: Vec<ExpansionVote>` to `NarrativePattern` — each entry is `{ parent_role: NodeRole, weight: i32 }` giving the pattern affinity for expanding slots of that role. |
| 5.2.2 | `PipelineConfig.expansion_budget` | ✅ | Add `expansion_budget: Option<u32>` to `PipelineConfig`. When `None`, derive from `MapScale` (Tiny=0, Small=1, Medium=2, Large=3, Huge=4). Allow override from `SituationContext.bindings["expansion_budget"]`. |
| 5.2.3 | Composition logic in filler | ✅ | Extend `fill_pattern` to accept `&[NarrativePattern]` (full library) + `expansion_budget: u32`. For expandable slots: (a) budget>0 check, (b) RNG inclusion roll, (c) filter library (exclude self, exclude patterns with required slots > remaining budget), (d) run `select_pattern` on filtered set (with expansion_votes as bonus), (e) recursively fill sub-pattern (decrement budget), (f) graft sub-graph into parent. |
| 5.2.4 | Graph grafting helper | ✅ | `graft_subgraph(parent_nodes, parent_edges, slot_key, sub_graph, next_id)` — merges sub-pattern nodes (with key namespacing: `slot_key.sub_key`), rewires parent edges pointing at slot to sub-pattern's Entry node, re-numbers node IDs for uniqueness. |
| 5.2.5 | Wire into `GenericIntentBuilder` | ✅ | Pass pattern library + budget to the filler. Budget derived from config or scale. |
| 5.2.6 | Annotate `narrative.json` | ✅ | Mark selected slots as `expandable: true` in existing patterns: `branching_exploration.branch_a`, `branching_exploration.branch_b`, `hub_and_spoke.spoke_1`, `lock_and_key.key_area`. Optionally add `expansion_votes` to patterns (e.g., `linear_descent` gets `[{ parent_role: "Branch", weight: 2 }]`). |
| 5.2.7 | Tests | ✅ | Unit: expandable deserializes, budget=0 prevents expansion, budget=1 expands one slot, composed graph edges valid, key namespacing correct. Proptest: any expandable pattern + vocabulary produces connected graph with valid edges. Integration: medium-scale situation produces 8–15 room graph. |

**5.2 design decisions:**
- **Voting over hard-coded IDs.** Sub-pattern selection reuses `select_pattern` — situation tags guide composition contextually. Adding a new pattern to the library automatically makes it eligible for expansion without editing existing patterns.
- **`expansion_votes` is optional, not required.** Without it, pure situation-tag voting works. The field exists for fine-tuning ("linear_descent prefers to expand Branch slots") without creating pattern-to-pattern coupling.
- **Self-exclusion prevents infinite recursion.** The current pattern (and its ancestors in the recursion stack) are excluded from sub-selection. Combined with budget, this guarantees termination.
- **Key namespacing preserves traceability.** A node `branch_a.hub.gate` tells you exactly where it came from in the composition tree. Annotations and debugging benefit from this.
- **Downstream is transparent.** The composed `ScenarioGraph` looks like any other graph to spatial/geometry/tile layers. No changes needed downstream — force-directed layout (5.1) handles the larger node counts.

**5.3 subtasks:**
| # | Subtask | Status | Description |
|---|---------|--------|-------------|
| 5.3.1 | `EntityCategory` enum | ✅ | `Creature`, `NPC`, `Hazard`, `Boss` — fixed enum for behavior dispatch (same pattern as `FeatureCategory`). |
| 5.3.2 | `EntityProperties` struct | ✅ | `name`, `category: EntityCategory`, `ascii_char: char`, `blocking: bool`, `tags: Vec<Tag>` — serde-serializable. |
| 5.3.3 | `EntityRegistry` struct | ✅ | `HashMap<String, EntityProperties>` with `default_registry()` (11 built-in types), `from_properties()`, `register()`, `get()`, `category()`, `ascii_char()`, `is_blocking()`, `len()`, `is_empty()`. |
| 5.3.4 | JSON asset `entity_types.json` | ✅ | `assets/rules/entity_types.json` — 11 entity archetypes (rat, tavern_rat, giant_rat, skeleton, skeleton_guardian, gate_guardian, chest_mimic, smuggler, bat, cave_spider, trap_mimic). Embedded fallback via `include_str!`. |
| 5.3.5 | Asset loader integration | ✅ | `EMBEDDED_ENTITY_REGISTRY`, `DEFAULT_ENTITY_REGISTRY_PATH`, `load_default_entity_registry()` in `asset/load.rs`. |
| 5.3.6 | Replace hardcoded `entity_char()` | ✅ | `ascii.rs` render functions accept `&EntityRegistry`. `entity_char()` delegates to `EntityRegistry::default_registry().ascii_char()`. Old substring matching removed. |
| 5.3.7 | `PipelineConfig.entity_registry` | ✅ | Optional override field (same pattern as `tile_registry` / `feature_registry`). |
| 5.3.8 | Tests | ✅ | 6 unit tests in `entity/registry.rs`, 1 asset loader test, existing ASCII render tests updated. All 425 tests green. |

**5.3 design decisions:**
- **HashMap-based (not index-based).** Unlike tiles (which need dense `u16` IDs for the tilemap), entities are looked up by string name. HashMap mirrors `FeatureRegistry` pattern.
- **Default char is `'?'` not `'E'`.** Unknown archetypes render as `?` — makes it obvious when an entity is missing from the registry, rather than silently rendering a generic placeholder.
- **`giant_rat` gets `'R'` (uppercase).** Distinguishes boss-tier entities visually in ASCII output.
- **New entities (bat, cave_spider) get dedicated chars.** `'b'` and `'x'` respectively — previously they fell through to the generic `'E'` in the old substring matcher.

**5.4 subtasks:**
| # | Subtask | Status | Description |
|---|---------|--------|-------------|
| 5.4.1 | `TensionLevel` enum | ✅ | `Low`, `Medium`, `High`, `Climax` with `Display` impl. Lives in `src/tension.rs`. |
| 5.4.2 | `compute_tension()` function | ✅ | Takes `&SpatialPlan`, returns `HashMap<SpaceId, TensionLevel>`. BFS from Entry, finds critical path (Entry→Goal), normalizes depth. Overrides: Goal→Climax, Entry→Low, Reward→Low. Thresholds: ≤0.25 Low, ≤0.60 Medium, ≤0.90 High, >0.90 Climax. |
| 5.4.3 | `RoomAnnotation.tension` field | ✅ | Added to `RoomAnnotation` struct. Computed in `build_annotations` via `compute_tension()`. |
| 5.4.4 | Annotated legend shows tension | ✅ | Format: `[A] Room Name (Role) [Tension] *`. Visible with `cargo run -- --annotate`. |
| 5.4.5 | Trace logging | ✅ | `--trace debug` logs each room's assigned tension level. |
| 5.4.6 | Unit tests | ✅ | 7 tests: linear path, short path, reward override, branch off critical path, no-goal fallback, display formatting, long gradual buildup. |

**5.4 design decisions:**
- **BFS distance, not path-following.** BFS gives the shortest distance from entry to each room regardless of the specific path taken. This handles branching naturally — a branch at depth 2 in a 4-deep graph gets Medium tension even though it's off the main line.
- **Critical path = Entry→Goal shortest path.** Not longest path (which would be confusing for players). The critical path is what a player would traverse if they went straight for the goal.
- **Reward rooms override to Low.** These are rest points by design. Even if they're at depth 3/4 of the critical path, they should feel safe.
- **No-goal fallback uses max BFS depth.** If there's no Goal room, the system still produces a gradual buildup across the map.
- **Tension is computed, not authored.** It's a structural property derived from graph topology, not a tag that content authors set. This means it stays correct even when pattern composition changes the graph shape.

**5.5 subtasks:**
| # | Subtask | Status | Description |
|---|---------|--------|-------------|
| 5.5.1 | `TensionLevel` gains `Ord` + `density_multiplier()` | ✅ | `PartialOrd`/`Ord` derived (Low < Medium < High < Climax). `density_multiplier()` returns 0.5/1.0/1.5/2.0. `Serialize`/`Deserialize` added. |
| 5.5.2 | `EntityRule.tension_min` field | ✅ | Optional `TensionLevel` (serde default None). Rule only fires in rooms at or above that tension level. `tension_allows()` helper method. |
| 5.5.3 | `matching_entity_rules` accepts tension | ✅ | Third parameter `Option<TensionLevel>` filters rules by tension. All callers updated. |
| 5.5.4 | Entity planner computes tension + scales density | ✅ | `SimpleEntityPlanner` calls `compute_tension(spatial)`, filters rules by room tension, scales `density_cap` by `tension.density_multiplier()`. Low-tension rooms get 0.5× entities; climax rooms get 2×. |
| 5.5.5 | Updated `entities.json` with tension gates | ✅ | Skeleton rules gated at Medium/High, bat/cave_spider at Medium/High, chest_mimic at Medium. Quest-critical entities (gate_guardian, skeleton_guardian) have no tension gate. 4 new rules added (total: 8). |
| 5.5.6 | Trace logging includes tension | ✅ | `--trace debug` shows room tension level in entity rule matching logs. |
| 5.5.7 | Tests | ✅ | 4 new planner tests (density scaling math, low-tension caps, high-tension allows more, tension_min filtering) + 5 new rule tests (tension_allows, serde round-trip, matching with tension). All 443 tests green. |

**5.5 design decisions:**
- **Density scaling via multiplier, not hard caps.** The base density cap (walkable_tiles/4) is multiplied by the tension level's multiplier. This means larger rooms still benefit from higher tension (more entities), while small rooms at low tension might only allow 1 entity.
- **`tension_min` is on the rule, not the entity type.** The same entity archetype (e.g. skeleton) can appear at different tension levels via different rules with different `tension_min` values. This allows "a few skeletons at Medium, extra aggressive skeletons at High."
- **Quest-critical entities bypass tension.** Gate guardians and goal guardians have no `tension_min` — they must appear regardless of where they fall on the pacing curve. The tension system scales *ambient* threat, not *structural* encounters.
- **Atmosphere per-room rules also respect tension.** When the atmosphere system injects entity rules for a specific room, those rules are also filtered by the room's tension level. This prevents atmosphere-driven rats from appearing in low-tension safe areas.
- **Minimum density cap is always 1.** Even at Low tension (0.5× multiplier), if a required entity needs placement, it can always fit.

---

### Phase 6: Urban Districts & Multi-Connector Spaces

**Goal:** Generate urban outdoor environments (streets, plazas, buildings) using the existing room-based architecture. The geometry layer gains alignment, multi-connector support, and edge zones. Streets become content-bearing navigational backbones, not just decorated corridors.

**The topological inversion:**
```
Dungeon:  NODES (rooms) are the content.   EDGES (corridors) are disposable glue.
Urban:    EDGES (streets) are the backbone. NODES (buildings) hang off them.
```

This is the fundamental shift. In a dungeon, you build rooms and connect them with corridors. In a district, you build streets and attach buildings to them. If you treat streets like "corridors but wider" you get decorated hallways pretending to be a city. Streets are high-branching, wide, content-heavy, and the navigational backbone — the opposite of dungeon corridors in every dimension except "connects two places."

**Key insight:** Narrative patterns still describe *why* you go places (story). Vocabulary describes *what* those places are (theme). Geometry describes *how* they're arranged (space). But in urban contexts, edges carry as much or more weight than nodes. Streets are **edges by default, nodes when promoted** — a quiet connector street is routing; an ambush street / checkpoint street / market chaos street is a `Transition` node with `archetype: Street`.

**The hierarchy emerges from existing layers:**
- **Vocabulary** says "Hub = Market Plaza" (gives the archetype)
- **Geometry** says "Plaza needs street connections, buildings align to streets" (spatial arrangement)
- **Composition** says "Goal(Warehouse) expands into interior rooms" (adds depth via 5.2)
- **Atmosphere** says "streets are busy" (populates routing and edge zones with content)

| # | Task | Status | Description |
|---|---|---|---|
| 6.1 | **Urban Space Archetypes & Edge Zones** | ✅ | New `SpaceArchetype` variants: `Plaza`, `Street`, `Alley`, `Shop`, `Warehouse`. `LocationKind::Urban` added. Extend the zone system with **edge zones** — spaces gain a periphery (`EdgeZone`) separate from interior. Streets: center zone (walkable path) + edge zones (where buildings attach, stalls spawn). Plazas: center open + edges populated. This makes feature placement and alignment saner than trying to scatter into a raw rect. |
| 6.2 | **Multi-Connector Routing & Distribution** | ✅ | Spaces can have N>2 doors. `SpaceSpec` gains `max_connectors: Option<u32>` and `connector_distribution: ConnectorDistribution`. Distribution enum: `Uniform` (evenly spaced along wall), `Clustered` (grouped at center), `GridAligned` (snapped to regular intervals), `Ends` (only at short sides — for street endpoints). Without distribution, you get "6 doors randomly slapped onto a wall" which looks like a panic attack, not a street. |
| 6.3 | **Alignment-Aware Geometry** | ✅ | Stronger constraints than just `FacesSpace`. New spatial constraints: `AlignEdge { a, b, side }` (building B's front wall aligns to street A's long edge), `AttachToEdge { building, street, side }` (building connects to a specific side of the street), `PreferOrientation { space, axis }` (streets prefer straight axes, plazas anchor intersections). Force-directed planner gains orientation forces, **but** a dedicated `StreetSkeletonPlanner` places streets first as axis-aligned backbone, then attaches buildings. Force-directed alone will fail at producing readable urban layouts. |

**6.3 Subtasks:**

| # | Subtask | Status | Description |
|---|---------|--------|-------------|
| 6.3.1 | **New `SpatialConstraint` variants** | ✅ | Add `AlignSide` enum (`North`, `South`, `East`, `West`), `Axis` enum (`Horizontal`, `Vertical`), and three new `SpatialConstraint` variants: `AlignEdge { a, b, side }` (space B's front wall aligns to space A's long edge), `AttachToEdge { building, street, side }` (building connects to a specific side of the street — implies adjacency + door placement), `PreferOrientation { space, axis }` (space should be elongated along the given axis). |
| 6.3.2 | **Automatic urban constraint inference** | ✅ | Extend spatial planner with `derive_urban_constraints()`: when `LocationKind::Urban`, auto-generate `PreferOrientation` for Street/Alley archetypes, `AttachToEdge` for Shop/Warehouse nodes linked to Street/Plaza nodes, `AlignEdge` for building-archetype nodes sharing a link with a street. |
| 6.3.3 | **`StreetSkeletonPlanner`** | ✅ | New `GeometryPlanner` impl in `src/geometry/street_skeleton.rs`. Algorithm: (1) partition spaces into backbone (Street/Alley/Plaza) vs attached (Shop/Warehouse/Chamber/etc), (2) place backbone first as axis-aligned rects (streets elongated, plazas square, at intersections), (3) attach buildings flush against backbone edges respecting `AttachToEdge`/`AlignEdge`, (4) resolve overlaps, (5) route with `ZShapeRouter`. |
| 6.3.4 | **`GeometryStrategy::StreetSkeleton` variant** | ✅ | Add new variant to `GeometryStrategy`. Wire into `Pipeline::plan_geometry_with_retries`. Add `--layout street` CLI option. Auto-select for `LocationKind::Urban` unless explicitly overridden. |
| 6.3.5 | **Orientation forces in force-directed planner** | ✅ | Extend `ForceDirectedGeometryPlanner` snap-to-grid: when a space has `PreferOrientation`, ensure its dimensions are oriented correctly (swap w/h if needed). Makes force-directed planner urban-aware as fallback. |
| 6.3.6 | **Unit tests** | ✅ | Tests: `AlignEdge` places building flush against street side; `AttachToEdge` produces door on correct face; `PreferOrientation` ensures correct elongation axis; `StreetSkeletonPlanner` produces valid non-overlapping layout for street+buildings graph; T-junction (plaza+3 streets); regression (existing scenarios unchanged). |

**6.3 New/Modified Files:**
- `src/spatial/plan.rs` — `AlignSide`, `Axis` enums; 3 new `SpatialConstraint` variants
- `src/spatial/planner.rs` — `derive_urban_constraints()` helper
- `src/geometry/street_skeleton.rs` — **new** `StreetSkeletonPlanner`
- `src/geometry/mod.rs` — `pub mod street_skeleton;`
- `src/geometry/force_directed.rs` — orientation-aware snap-to-grid
- `src/pipeline.rs` — `GeometryStrategy::StreetSkeleton`; auto-select for urban
- `src/main.rs` — `Layout::Street` CLI variant

**6.3 Acceptance Criteria:**
- A test urban `SpatialPlan` (1 plaza + 2 streets + 4 shops) produces non-overlapping `GeometryPlan` with buildings flush against street edges.
- Streets are axis-aligned (not diagonal/rotated).
- Buildings' connector (door) faces the street they're attached to.
- Existing demo scenarios produce identical results (regression).
- `cargo test` passes (all existing tests + new 6.3 tests).
| 6.4 | **Street Rasterization** | | Streets produce elongated footprints with implicit boundaries (not hard walls — edge zones define where the street ends and buildings begin). New tile: `Cobblestone`. Sidewalk zones (optional edge softness). Open ends connect to intersections. Connectors distributed along long sides via 6.2. T-junctions and crossroads emerge from multi-street intersections handled by the router. |
| 6.5 | **Buildings: Expansions + Facades** | | Two building modes: (a) **Expansion** — quest-relevant buildings expand via 5.2 composition into multi-room interiors (sub-pattern's Entry = street-facing door). (b) **Facade** — lightweight non-quest buildings as nodes with connector + exterior only (no interior, no expansion). Keeps street density believable without complexity explosion. Without facades, you get empty streets with one important door. `ExitMarker` on expansion slots signals "leads to another map" for large buildings. |
| 6.6 | **Urban Narrative Patterns** | | New story patterns for urban pacing: `urban_heist` (safehouse→market→fence→checkpoint→warehouse), `investigation` (crime scene→witnesses→suspect locations→confrontation), `chase` (origin→street→alley→rooftops→dead end). Streets appear as `Transition` nodes when they're gameplay-relevant (ambush, checkpoint, chase). Quiet connector streets stay as edges (routing). |
| 6.7 | **Urban Content Rules** | | New feature types: `street_lamp`, `market_stall`, `well`, `signpost`, `cart`, `crate_stack`. Entity rules: `guard` (patrol on streets), `merchant` (near stalls in edge zones), `beggar` (alleys). Atmosphere profiles: `busy_market`, `seedy_docks`, `quiet_residential`. `PlacementStrategy::AlongAxis` for linear spaces. `PlacementStrategy::InEdgeZone` for periphery content. |
| 6.8 | **Motif Propagation** | | Motifs gain spatial spread: a motif placed on one node propagates along edges with decay. `MotifField { source_role, decay_per_hop, leak_probability }`. Atmosphere profiles already react to tags — this adds tag *propagation* before profile matching. Enables "rat infestation spreading from warehouse along dock streets, leaking into adjacent basements." |
| 6.9 | **New Scenario: Port District** | | Streets + docks + warehouses + tavern. Tests: multi-connector streets (5+ doors with uniform distribution), building expansion + facade density, street features in edge zones, alignment (buildings face streets). Motif: rat infestation originating from warehouse, spreading along dock streets. `PinEntity { "Rat King", target: Goal }`. 15–25 spaces total. |

**Implementation Strategy: Street-Skeleton-First with Frontage Parcels**

The core algorithm — minimal convincing urban generation in one sentence: *use a street-skeleton-first generator with frontage zones, parcel subdivision, and building facades/interior expansion.* This produces readable urban form without needing full city simulation.

**Pipeline:**
```
place_anchors → build_street_skeleton → realize_streets → compute_frontage_zones → subdivide_parcels → assign_use → place_buildings → UrbanPlan → GeometryPlan
```

**Step 1 — Place Anchors.** Anchors are *reasons for streets to exist*: entry gate, market plaza, dock, warehouse, tavern, checkpoint, goal building, side alley exit. Each anchor has a kind, weight, and optional zone hint (e.g. plaza near center, docks on water edge, warehouse near docks). This drives the skeleton — streets don't exist arbitrarily, they connect motivating places.

```rust
struct UrbanAnchor {
    id: AnchorId,
    kind: AnchorKind,
    weight: f32,
    preferred_zone: Option<ZoneHint>,
}
```

**Step 2 — Generate Street Skeleton.** An axis-aligned graph connecting anchors. Start simple: main street (entry → plaza → docks), side street (plaza → warehouse), alley (tavern → back route → goal). This is intentionally not fancy — connectivity and hierarchy matter more than organic curves at this stage.

```rust
fn build_street_skeleton(anchors: &[UrbanAnchor], rng: &mut Rng) -> StreetGraph {
    let mut graph = StreetGraph::new();
    // Main spine: entry → plaza → docks
    graph.add_street(entry, plaza, StreetKind::Main);
    graph.add_street(plaza, docks, StreetKind::Main);
    // Side streets to important non-main anchors
    for important in important_non_main_anchors() {
        graph.add_street(plaza, important, StreetKind::Side);
    }
    // Optional alleys for alternative routing
    add_optional_alleys(&mut graph, rng);
    graph
}
```

**Step 3 — Realize Streets as Width-Bearing Segments.** Each skeleton edge becomes a wide rectangle with kind-dependent width:
- Main street: 5–7 tiles
- Side street: 3–5 tiles
- Alley: 1–2 tiles

Rasterization: center = walkable cobblestone, edges = sidewalk / frontage zones.

```rust
struct StreetSegment {
    from: Point,
    to: Point,
    width: i32,
    kind: StreetKind,
}
```

**Step 4 — Compute Frontage Zones.** For each street segment, compute buildable strips along both sides. This is the *secret sauce* — frontage zones are where buildings attach, facing the street.

```
████ buildings
.... edge zone / sidewalk
==== street center
.... edge zone / sidewalk
████ buildings
```

```rust
struct FrontageZone {
    street_id: StreetId,
    side: Side,
    rect: Rect,
    allowed_uses: Vec<BuildingUse>,
}
```

**Step 5 — Subdivide Frontage into Parcels.** Take each frontage strip and split into lots (width 4–9 tiles). This produces believable urban rhythm — irregular but coherent lot widths. Crucially: parcels *face the street*, so doors go on the frontage side and buildings align automatically.

```rust
fn subdivide_frontage(zone: &FrontageZone, rng: &mut Rng) -> Vec<Parcel> {
    let mut parcels = Vec::new();
    let mut cursor = zone.start();
    while cursor < zone.end() {
        let width = rng.range(4..9);
        parcels.push(Parcel {
            frontage: Rect::from_cursor(cursor, width, zone.depth),
            street_id: zone.street_id,
            side: zone.side,
        });
        cursor += width;
    }
    parcels
}
```

**Step 6 — Assign Parcel Use.** Context-driven: near plaza → shops, tavern, market stalls; near docks → warehouse, storage, cheap tavern; side alleys → residences, shady doors; goal parcel → quest building. Uses weighted picking based on proximity to anchors.

**Step 7 — Place Buildings.** Each parcel becomes either a **facade** (1–3 tiles deep, door on street side, no interior) or an **expanded interior** (parcel footprint becomes building shell, inside runs existing room composition / pattern expansion via 5.2). Door alignment is trivial because the parcel knows its street-facing edge.

**What makes it convincing:** Not randomness — *alignment and frontage.* Streets first, buildings facing streets, doors on frontage, plazas at intersections, shops near plazas, warehouses near docks, alleys thinner and less regular, background facades for density, important buildings expanded into interiors.

**Minimal starting scope:**
1. One main street
2. One plaza
3. One dock/warehouse branch
4. Frontage parcels
5. Facade buildings
6. One expanded quest building

Everything else (motif propagation, chase sequences, complex narrative patterns) layers on top of this foundation.

**How this maps to tasks:**
- Steps 1–2 implement task 6.3 (StreetSkeletonPlanner + anchor placement)
- Step 3 implements task 6.4 (street rasterization with width hierarchy)
- Step 4 implements task 6.1 (edge zones = frontage zones)
- Step 5 is new machinery — parcel subdivision produces the "urban rhythm" that makes districts convincing
- Step 6 connects to task 6.7 (content rules) and atmosphere profiles
- Step 7 implements task 6.5 (expansion + facades), with connector distribution (6.2) governing door placement along frontage

---

**Design decisions:**
- **Topological inversion is real but doesn't require new abstractions.** Streets as `Transition` nodes + rich routing = the same graph model, different spatial weight. The geometry planner treats streets as primary layout elements (placed first), buildings as secondary (attached after).
- **Streets are edges by default, nodes when promoted.** Quiet connector streets are routing (corridor equivalent). Important gameplay streets (ambush, checkpoint, market chaos, chase sequence) are `Transition` nodes with `archetype: Street`. No new type needed.
- **Connector distribution prevents visual chaos.** `max_connectors` alone is necessary but insufficient. Without `ConnectorDistribution` you get doors slapped randomly. With it, streets have evenly-spaced building entrances, plazas have clustered access points, alleys have connectors only at ends.
- **Alignment needs more than force-directed.** `FacesSpace` is a start, but buildings lining up along streets requires explicit axis constraints (`AlignEdge`, `AttachToEdge`, `PreferOrientation`). A `StreetSkeletonPlanner` places streets first as axis-aligned bones, then force-directed attaches buildings. Pure force-directed will produce organic-looking mess, not readable urban grid.
- **Edge zones make feature placement sane.** Without them, you're scattering features into a raw rect and hoping they look like a street market. With center/edge zone distinction, stalls go in edge zones, the path stays clear, buildings attach at the periphery.
- **Facades for density.** If only quest buildings are nodes, streets feel empty. Lightweight facade nodes (connector + exterior wall, no interior) provide visual density without expansion cost. A facade is a 1-cell-deep wall with a door that goes nowhere (or leads to ExitMarker for future maps).
- **Building interiors reuse 5.2 composition.** A `Goal(Warehouse)` slot expands into a `lock_and_key` interior. Zero new composition machinery needed.
- **6.1–6.3 are the foundation** (edge zones + multi-connector + alignment). Without them, everything collapses into spaghetti.
- **6.4–6.7 are incremental content** (rasterization, buildings, patterns, features). Can be added one at a time.
- **6.8 is the deferred motif propagation** from Phase 4.6 — now with a real scenario that needs it.

**Exit criterion:** `cargo run -- port_district` produces a connected urban map where streets are visually recognizable as streets (long, axis-aligned, buildings lined up on both sides with evenly-distributed doors), plazas anchor intersections, quest buildings have interiors, facade buildings provide density, and motif-driven entity spread creates coherent themed areas. The narrative pattern is recognizable as a heist/investigation story structure realized in urban geometry.

---

### Phase 7: Organic Caves & Pure Outdoors

**Goal:** Large cave halls gain interior structure (marketplace-like feature arrangement). Pure outdoor maps introduce area-scale planning (regions, paths, landmarks) above the room-scale system.

**Key insight:** Phase 6's open-space and alignment work directly benefits caves (a large hall IS an underground plaza). Pure outdoors is a genuinely new abstraction layer — regions connected by organic paths, not corridors.

| # | Task | Status | Description |
|---|---|---|---|
| 7.1 | **Cave Halls as Structured Spaces** | | Large cave rooms (≥10×10) gain interior structure using the zone+template system. A "fungal market" template arranges stalagmite clusters like stalls. An "underground lake" template carves a water region with shore paths. Existing `CaveIrregularizer` provides shape; new templates provide layout. |
| 7.2 | **Area Plan (Region-Scale Layout)** | | New layer above `MapIntent`: `AreaPlan` defines regions (forest, cliff, cave entrance, clearing) with transitions. Each region becomes one or more `MapIntent` instances composed together. The pipeline gains an optional area-planning stage. |
| 7.3 | **Organic Connections** | | Connections between outdoor regions aren't hallways — they're paths, rivers, ridgelines. New `ConnectionStyle` enum: `Corridor` (existing), `Path` (wider, irregular edges), `River` (water tiles, bridges), `Cliff` (vertical, stairs). Router selects style from region adjacency. |
| 7.4 | **Biome Transitions** | | Where two regions meet, a transition zone blends their features. Forest→cliff: trees thin, rocks appear. Cave→outdoors: daylight scatter, moss. Implemented as atmosphere profiles keyed to `(region_a, region_b)` pairs applied to border spaces. |
| 7.5 | **Landmark Planning** | | `AreaPlan` gains `landmarks: Vec<Landmark>` — notable features visible from a distance (tower, ancient tree, waterfall). Landmarks anchor navigation; the area planner places regions *around* landmarks. They become Goal/Hub nodes in the area-level graph. |
| 7.6 | **New Scenario: Mountain Pass** | | Outdoor trail: forest clearing → mountain path → cliff overlook → cave entrance → cave interior. Tests: organic connections (paths not corridors), biome transitions, 2 map scales (area + room), landmark (the peak visible throughout). |

**Design decisions:**
- 7.1 is a quick win: just adds templates for existing large cave rooms. The zone system already supports this.
- 7.2–7.3 are the big architectural leap: a hierarchical pipeline where the area plan generates multiple `MapIntent`s. Genuinely new structure.
- This phase is intentionally open-ended — scope to what the Mountain Pass scenario needs, defer the rest.
- Multi-level support (Wizard's Tower) can slot in here as vertical connections between area regions.

**Exit criterion:** `cargo run -- mountain_pass` produces a multi-region outdoor map with organic path connections, biome blending at region borders, and at least one landmark that anchors navigation.

---

### Scenario Roadmap

| Scenario | Key Tests | Unlocked by |
|---|---|---|
| Noble Crypt | lock-and-key, undead theme | Foundation ✅ |
| Tavern Cellar | branching, urban theme, secrets | Foundation ✅ |
| Natural Cave | exploration, scatter terrain, irregular shapes, pattern composition | Phase 3 ✅ (shapes: Phase 4, composition: Phase 5.2) |
| Rat-Infested Port Cellar | motif directives, entity pressure, infested rooms | Phase 4 ✅ |
| Abandoned Mine | medium scale, pacing, force-directed layout | Phase 5 |
| **Port District** | multi-connector streets, building expansion, outdoor features, motif propagation | **Phase 6** |
| Thieves' Guild | complex hub-and-spoke, traps, heist pattern (variant of Port District) | Phase 6 |
| **Mountain Pass** | organic connections, biome transitions, area-scale planning, landmarks | **Phase 7** |
| Wizard's Tower | vertical traversal, multi-level, arcane theme | Phase 7 (vertical as connection style) |
| Dragon's Lair | large scale, landmark-driven, boss encounter | Phase 7 |

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
